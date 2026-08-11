//! Applying caller-supplied field changes to an entity.
//!
//! `keel_update(id, version, changes: {...})` hands over an arbitrary JSON
//! object. Turning that into a validated, typed mutation for thirteen
//! different structs is the sort of job that invites thirteen hand-written
//! setter functions, each with its own opportunity to forget a field.
//!
//! Instead the entity is round-tripped through its own `serde` representation:
//! serialise, merge the changes, deserialise. That means every field is
//! covered automatically and — more usefully — `serde`'s own errors already
//! read the way the MCP contract demands. "unknown variant `dun`, expected one
//! of `todo`, `in_progress`, `blocked`, `review`, `done`, `wont_do`" is
//! precisely what an agent needs to retry successfully.

use crate::{Entity, Error, Result};
use serde_json::{Map, Value};

/// Fields a caller may never set directly, and why.
///
/// Each of these is owned by the system rather than the caller. Silently
/// ignoring an attempt would be worse than rejecting it: an agent that thinks
/// it moved a task to another project, and is not told otherwise, will build
/// on that belief.
const IMMUTABLE: &[(&str, &str)] = &[
    ("type", "the artifact type is fixed at creation"),
    ("id", "identifiers are permanent"),
    ("audit", "provenance is recorded by the store, not supplied"),
    (
        "idempotency_key",
        "the key is set at creation and is what makes retries safe",
    ),
    (
        "project_id",
        "an artifact cannot move between projects; archive it and create it in the other project",
    ),
    (
        "metric_id",
        "an observation belongs to the metric it was recorded against",
    ),
    (
        "current_doc_version",
        "the revision pointer is advanced by keel_write_doc, not by an update",
    ),
    (
        "number",
        "a task's number is assigned in creation order and never reused, so that \
         `KEEL-42` means the same task forever",
    ),
];

/// One field's transition, for the event log.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldChange {
    /// The field name, as the caller wrote it.
    pub field: String,
    /// The value before.
    pub before: Value,
    /// The value after.
    pub after: Value,
}

/// Apply `changes` to `entity`, returning what actually changed.
///
/// A change whose new value equals the old one is dropped rather than
/// recorded: it produces no event, and an activity feed full of "status
/// changed from done to done" is worse than useless.
///
/// Returns an empty vector when nothing changed, which callers should treat as
/// a successful no-op rather than an error — a retrying agent re-sending the
/// same update should not be punished for it.
pub fn apply_changes(
    entity: &mut Entity,
    changes: &Map<String, Value>,
) -> Result<Vec<FieldChange>> {
    let entity_type = entity.entity_type();

    let Value::Object(mut current) =
        serde_json::to_value(&*entity).map_err(Error::json(format!(
            "read the current state of {} before updating it",
            entity.id()
        )))?
    else {
        return Err(Error::Invariant {
            operation: format!("update {}", entity.id()),
            problem: "the entity did not serialise to an object".to_owned(),
        });
    };

    let mut applied = Vec::new();
    for (field, new_value) in changes {
        if let Some((_, why)) = IMMUTABLE.iter().find(|(name, _)| name == field) {
            return Err(Error::invalid(
                entity_type,
                field,
                format!("`{field}` cannot be changed: {why}"),
                format!("any of: {}", settable_fields(&current).join(", ")),
            ));
        }

        let Some(old_value) = current.get(field) else {
            return Err(Error::invalid(
                entity_type,
                field,
                format!("{entity_type} has no field `{field}`"),
                format!("any of: {}", settable_fields(&current).join(", ")),
            ));
        };

        if old_value == new_value {
            continue;
        }

        applied.push(FieldChange {
            field: field.clone(),
            before: old_value.clone(),
            after: new_value.clone(),
        });
        current.insert(field.clone(), new_value.clone());
    }

    if applied.is_empty() {
        return Ok(applied);
    }

    // Deserialising is where type and enum validation actually happens. The
    // error is rewritten to name the offending field, because serde's own
    // message says what was wrong but not always where.
    let updated: Entity = serde_json::from_value(Value::Object(current)).map_err(|e| {
        let offending = applied
            .iter()
            .map(|c| c.field.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        Error::invalid(
            entity_type,
            offending,
            e.to_string(),
            "a value matching the field's declared type; enum fields list their \
             permitted values in the message above"
                .to_owned(),
        )
    })?;

    *entity = updated;
    Ok(applied)
}

/// The field names a caller may set, for error messages.
fn settable_fields(current: &Map<String, Value>) -> Vec<&str> {
    current
        .keys()
        .map(String::as_str)
        .filter(|k| !IMMUTABLE.iter().any(|(name, _)| name == k))
        .collect()
}

/// Whether a change touches the `status` field, which gets its own event
/// action so the activity feed and roadmap can filter on it cheaply.
pub fn is_status_change(changes: &[FieldChange]) -> bool {
    changes
        .iter()
        .any(|c| c.field == "status" || c.field == "state")
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::{EntityId, EntityType, Task, TaskStatus};
    use serde_json::json;

    fn task() -> Entity {
        Task::new(
            EntityId::generate(EntityType::Project),
            "Ship the daemon",
            "A row this test needs in the store.",
        )
        .into()
    }

    fn changes(v: Value) -> Map<String, Value> {
        match v {
            Value::Object(m) => m,
            _ => Map::new(),
        }
    }

    #[test]
    fn a_valid_change_is_applied_and_reported() {
        let mut e = task();
        let applied = apply_changes(&mut e, &changes(json!({"status": "in_progress"}))).unwrap();

        assert_eq!(applied.len(), 1);
        assert_eq!(applied[0].field, "status");
        assert_eq!(applied[0].before, json!("todo"));
        assert_eq!(applied[0].after, json!("in_progress"));

        let Entity::Task(t) = e else {
            panic!("still a task")
        };
        assert_eq!(t.status, TaskStatus::InProgress);
    }

    #[test]
    fn several_fields_change_at_once() {
        let mut e = task();
        let applied = apply_changes(
            &mut e,
            &changes(json!({"status": "done", "priority": "p0", "labels": ["infra"]})),
        )
        .unwrap();
        assert_eq!(applied.len(), 3);

        let Entity::Task(t) = e else { panic!() };
        assert_eq!(t.status, TaskStatus::Done);
        assert_eq!(t.labels, vec!["infra"]);
    }

    #[test]
    fn a_no_op_change_produces_no_event() {
        let mut e = task();
        let applied = apply_changes(&mut e, &changes(json!({"status": "todo"}))).unwrap();
        assert!(
            applied.is_empty(),
            "re-sending the current value must not fill the feed with noise"
        );
    }

    #[test]
    fn an_invalid_enum_value_names_the_alternatives() {
        let mut e = task();
        let err = apply_changes(&mut e, &changes(json!({"status": "dun"})))
            .unwrap_err()
            .to_string();
        assert!(err.contains("status"), "should name the field: {err}");
        assert!(
            err.contains("in_progress") || err.contains("wont_do"),
            "should list what would have been valid: {err}"
        );
    }

    #[test]
    fn a_rejected_change_leaves_the_entity_untouched() {
        let mut e = task();
        let before = e.clone();
        let _ = apply_changes(&mut e, &changes(json!({"status": "dun"})));
        assert_eq!(e, before, "a failed update must not partially apply");
    }

    #[test]
    fn unknown_fields_are_rejected_rather_than_ignored() {
        let mut e = task();
        let err = apply_changes(&mut e, &changes(json!({"asignee": "kb"})))
            .unwrap_err()
            .to_string();
        assert!(err.contains("asignee"), "{err}");
        assert!(err.contains("title"), "should list real fields: {err}");
    }

    #[test]
    fn immutable_fields_are_rejected_with_the_reason() {
        for (field, value) in [
            ("id", json!("tsk_01H8XK4RPVBQ2N7DZM9C3FGTWY")),
            ("project_id", json!("prj_01H8XK4RPVBQ2N7DZM9C3FGTWY")),
            ("type", json!("bug")),
            ("idempotency_key", json!("abc")),
            ("current_doc_version", json!(4)),
        ] {
            let mut e = task();
            let err = apply_changes(&mut e, &changes(json!({field: value})));
            let err = match err {
                Err(err) => err.to_string(),
                Ok(_) => panic!("`{field}` should not be settable"),
            };
            assert!(err.contains(field), "{err}");
        }
    }

    #[test]
    fn moving_a_task_between_projects_says_what_to_do_instead() {
        let mut e = task();
        let err = apply_changes(
            &mut e,
            &changes(json!({"project_id": "prj_01H8XK4RPVBQ2N7DZM9C3FGTWY"})),
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("archive it and create it"), "{err}");
    }

    #[test]
    fn status_changes_are_detectable_for_both_naming_conventions() {
        assert!(is_status_change(&[FieldChange {
            field: "status".into(),
            before: json!("a"),
            after: json!("b")
        }]));
        // Design artifacts call it `state`.
        assert!(is_status_change(&[FieldChange {
            field: "state".into(),
            before: json!("proposed"),
            after: json!("built")
        }]));
        assert!(!is_status_change(&[FieldChange {
            field: "title".into(),
            before: json!("a"),
            after: json!("b")
        }]));
    }

    #[test]
    fn every_entity_type_can_be_patched() {
        use crate::*;
        let p = EntityId::generate(EntityType::Project);
        let m = EntityId::generate(EntityType::Metric);
        let cases: Vec<(Entity, &str, Value)> = vec![
            (Project::new("k", "Keel").into(), "name", json!("Keel v2")),
            (
                Milestone::new(p.clone(), "P0", "The first phase, for a store test.").into(),
                "status",
                json!("active"),
            ),
            (
                Task::new(p.clone(), "t", "A row this test needs in the store.").into(),
                "status",
                json!("done"),
            ),
            (
                Spec::new(p.clone(), "s").into(),
                "status",
                json!("approved"),
            ),
            (
                Decision::new(p.clone(), "d").into(),
                "status",
                json!("accepted"),
            ),
            (
                Question::new(p.clone(), "q").into(),
                "status",
                json!("answered"),
            ),
            (
                Term::new(Some(p.clone()), "t", "d").into(),
                "definition",
                json!("new"),
            ),
            (Feedback::new(p.clone(), "f").into(), "triaged", json!(true)),
            (Design::new(p.clone(), "d").into(), "state", json!("built")),
            (
                Environment::new(p.clone(), "prod").into(),
                "status",
                json!("healthy"),
            ),
            (
                Metric::new(p.clone(), "m").into(),
                "target_value",
                json!(0.9),
            ),
            (
                MetricObservation::new(m, p.clone(), 1.0, chrono::Utc::now()).into(),
                "note",
                json!("spike"),
            ),
            (
                Artifact::new(p, "a").into(),
                "url",
                json!("https://example.com"),
            ),
        ];
        assert_eq!(cases.len(), 13);

        for (mut entity, field, value) in cases {
            let ty = entity.entity_type();
            let applied = apply_changes(&mut entity, &changes(json!({field: value})))
                .unwrap_or_else(|e| panic!("{ty}.{field} should be settable: {e}"));
            assert_eq!(applied.len(), 1, "{ty}.{field} did not change");
        }
    }
}
