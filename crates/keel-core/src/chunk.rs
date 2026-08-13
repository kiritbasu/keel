//! Splitting a document into passages small enough for the model to read.
//!
//! `bge-small-en-v1.5` reads 512 tokens and a document used to go to it whole,
//! so 41% of the live store's current documents were embedded on their opening
//! paragraph and nothing said so — the technical specification got its first
//! 2.5%. B-55 settles the shape: headings first, then a hard wrap, with the
//! heading path carried into each passage's text.
//!
//! # Why the heading path travels with the passage
//!
//! A passage lifted out of §5 of the specification reads as prose about
//! caching or downloads and says nothing about Keel, embeddings or the
//! document it came from. Embedded on its own it answers questions nobody
//! asked. Prefixed with `Keel — Technical Specification › Storage ›
//! Embeddings` it keeps the context that the heading structure already
//! encoded, which is free — the author wrote those headings — and is most of
//! the value on a corpus this structured.
//!
//! # Why the numbers are what they are
//!
//! [`MAX_CHARS`] is 1,400 against a window of roughly 1,700, measured at about
//! 3.3 characters per token on this corpus's markdown. The gap is deliberate
//! slack: the ratio is an average over prose, and a passage dense in code,
//! tables or ids tokenises far worse than one of flowing English. Sizing to
//! the estimate exactly would truncate the passages that are least like the
//! text the estimate was measured on, which is the wrong tail to lose.
//!
//! [`OVERLAP_CHARS`] is 15%, which exists so a sentence that happens to fall
//! across a wrap is still wholly present in one of the two passages. Without
//! it, the one idea that spans a boundary is the one idea that becomes
//! unfindable, and boundaries land where the character count runs out rather
//! than anywhere meaningful.
//!
//! # Fences
//!
//! A `#` inside a fenced code block is a comment, a shell prompt or a colour,
//! not a heading. This file's own corpus is full of them. Tracking fence state
//! is not defensive: without it, a bash block in the specification opens a
//! phantom section, and every passage after it inherits a heading path built
//! from a comment.

/// The most characters a passage may carry, before the heading path is added.
///
/// See the module docs for why this is comfortably under the window rather
/// than at it.
pub const MAX_CHARS: usize = 1_400;

/// How much of the previous passage the next one repeats.
///
/// 15% of [`MAX_CHARS`]. Enough for a sentence that straddles a wrap to be
/// whole somewhere.
pub const OVERLAP_CHARS: usize = MAX_CHARS * 15 / 100;

/// A passage of a document, and where it came from.
///
/// `start` and `end` are byte offsets into the body it was cut from, so a
/// caller can go back to the source text rather than trusting the copy. They
/// are always on character boundaries — see [`floor_char_boundary`], which is
/// the whole reason this is not a plain `body[a..b]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chunk {
    /// Position within the document, from zero. Stable for a given body.
    pub ordinal: usize,
    /// The headings above this passage, outermost first, joined by ` › `.
    /// Empty for a document with no headings at all.
    pub heading_path: String,
    /// Byte offset of the passage's first character in the body.
    pub start: usize,
    /// Byte offset one past the passage's last character in the body.
    pub end: usize,
    /// The passage itself, trimmed.
    pub text: String,
}

impl Chunk {
    /// What actually goes to the model.
    ///
    /// Title, then the heading path, then the passage. Built here rather than
    /// at the call sites because the write path and the re-embed pass must
    /// produce byte-identical text for the same passage — otherwise a
    /// backfilled vector and a freshly written one are not comparable, and
    /// which one a document has comes down to when it was last touched. That
    /// is the same reasoning as `Document::searchable_text_of`, and the same
    /// trap.
    pub fn embed_text(&self, title: &str) -> String {
        if self.heading_path.is_empty() {
            format!("{title}\n\n{}", self.text)
        } else {
            format!("{title} › {}\n\n{}", self.heading_path, self.text)
        }
    }
}

/// Split a document body into passages.
///
/// Sections first, on markdown headings, so a passage boundary lands where the
/// author put one wherever possible. Sections longer than [`MAX_CHARS`] are
/// then wrapped, preferring a paragraph break, then a line break, then
/// whitespace, and only cutting mid-word when a single run of text offers
/// nothing better.
///
/// Returns an empty vector for a body with nothing in it but whitespace. That
/// is not a failure: a spec created as a title with the prose still to come is
/// an ordinary thing, and it has no passages until it has words.
pub fn split(body: &str) -> Vec<Chunk> {
    let mut chunks = Vec::new();
    for (heading_path, start, end) in sections(body) {
        let section = &body[start..end];
        for (rel_start, rel_end) in wrap(section) {
            let text = section[rel_start..rel_end].trim();
            if text.is_empty() {
                continue;
            }
            // Trimming moved the boundaries, so recover them rather than
            // reporting a span whose ends are whitespace the text does not
            // include. An offset that is nearly right is worse than none: it
            // reads as usable.
            let lead =
                section[rel_start..rel_end].len() - section[rel_start..rel_end].trim_start().len();
            chunks.push(Chunk {
                ordinal: chunks.len(),
                heading_path: heading_path.clone(),
                start: start + rel_start + lead,
                end: start + rel_start + lead + text.len(),
                text: text.to_owned(),
            });
        }
    }
    chunks
}

/// Cut the body at headings, returning `(heading path, start, end)` per section.
///
/// The text before the first heading is its own section with an empty path,
/// which is where a document that has no headings at all ends up entire.
fn sections(body: &str) -> Vec<(String, usize, usize)> {
    let mut out: Vec<(String, usize, usize)> = Vec::new();
    let mut stack: Vec<(usize, String)> = Vec::new();
    let mut current_start = 0usize;
    let mut current_path = String::new();
    let mut in_fence = false;
    let mut offset = 0usize;

    for line in body.split_inclusive('\n') {
        let trimmed = line.trim_start();
        // ``` or ~~~, and the closing fence is any line that opens one too.
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_fence = !in_fence;
            offset += line.len();
            continue;
        }
        if !in_fence && let Some(level) = heading_level(trimmed) {
            if offset > current_start {
                out.push((current_path.clone(), current_start, offset));
            }
            let title = trimmed[level..].trim().to_owned();
            stack.retain(|(l, _)| *l < level);
            stack.push((level, title));
            current_path = stack
                .iter()
                .map(|(_, t)| t.as_str())
                .collect::<Vec<_>>()
                .join(" › ");
            current_start = offset + line.len();
        }
        offset += line.len();
    }
    if body.len() > current_start {
        out.push((current_path, current_start, body.len()));
    }
    out
}

/// How many leading `#` a line has, if it is a heading.
///
/// `#` up to six, then a space — `#hashtag` is not a heading and neither is
/// `#######`. This is CommonMark's rule and it matters here because ids and
/// anchors in this corpus start with `#`.
fn heading_level(trimmed: &str) -> Option<usize> {
    let hashes = trimmed.bytes().take_while(|b| *b == b'#').count();
    if (1..=6).contains(&hashes) && trimmed.as_bytes().get(hashes) == Some(&b' ') {
        Some(hashes)
    } else {
        None
    }
}

/// Wrap one section into `(start, end)` spans no longer than [`MAX_CHARS`].
///
/// Byte spans, and every boundary is nudged to a character boundary before it
/// is used — slicing a `&str` anywhere else panics, and the panic would arrive
/// from whichever document first contained an em-dash near a wrap.
fn wrap(section: &str) -> Vec<(usize, usize)> {
    if section.len() <= MAX_CHARS {
        return vec![(0, section.len())];
    }
    let mut spans = Vec::new();
    let mut start = 0usize;
    while start < section.len() {
        let hard = floor_char_boundary(section, (start + MAX_CHARS).min(section.len()));
        if hard >= section.len() {
            spans.push((start, section.len()));
            break;
        }
        let end = break_before(section, start, hard);
        spans.push((start, end));
        // Step back by the overlap, but never far enough to stand still — a
        // window that does not advance is an infinite loop that fills the disk
        // before anyone sees a symptom.
        let next = end.saturating_sub(OVERLAP_CHARS).max(start + 1);
        start = floor_char_boundary(section, next);
    }
    spans
}

/// The best place to end a passage that must end somewhere before `hard`.
///
/// A paragraph break, then a line break, then any whitespace, then `hard`
/// itself. Only the last quarter of the window is searched: a paragraph break
/// near the *start* of the window is a worse boundary than a mid-sentence one
/// near the end, because it throws away most of the passage.
fn break_before(section: &str, start: usize, hard: usize) -> usize {
    let floor = start + (hard - start) * 3 / 4;
    let window = &section[start..hard];
    for pattern in ["\n\n", "\n", " "] {
        if let Some(found) = window.rfind(pattern) {
            let at = start + found + pattern.len();
            if at > floor {
                return floor_char_boundary(section, at);
            }
        }
    }
    hard
}

/// The largest character boundary at or below `index`.
///
/// `str::floor_char_boundary` is unstable, and the alternative is a slice that
/// panics on any multi-byte character near a wrap. This corpus is full of `›`,
/// `—` and `§`, so that is not a theoretical concern — it is the first
/// document anyone tries.
fn floor_char_boundary(s: &str, index: usize) -> usize {
    let mut i = index.min(s.len());
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn a_short_document_is_one_passage() {
        let chunks = split("Just a sentence.");
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].text, "Just a sentence.");
        assert_eq!(chunks[0].heading_path, "");
    }

    #[test]
    fn whitespace_only_produces_nothing() {
        assert!(split("   \n\n  \n").is_empty());
        assert!(split("").is_empty());
    }

    #[test]
    fn headings_become_the_path_and_nest() {
        let body = "# Top\n\nIntro.\n\n## Middle\n\nBody.\n\n### Deep\n\nLeaf.\n";
        let chunks = split(body);
        let paths: Vec<&str> = chunks.iter().map(|c| c.heading_path.as_str()).collect();
        assert_eq!(paths, vec!["Top", "Top › Middle", "Top › Middle › Deep"]);
    }

    /// Coming back out to a shallower heading has to drop the deeper ones.
    #[test]
    fn a_shallower_heading_pops_the_stack() {
        let body = "# A\n\nx\n\n## B\n\ny\n\n### C\n\nz\n\n## D\n\nw\n";
        let paths: Vec<String> = split(body).into_iter().map(|c| c.heading_path).collect();
        assert_eq!(paths, vec!["A", "A › B", "A › B › C", "A › D"]);
    }

    /// The bug this would have had: a `#` in a bash block opening a section.
    #[test]
    fn a_hash_inside_a_fence_is_not_a_heading() {
        let body = "# Real\n\nprose\n\n```bash\n# not a heading\necho hi\n```\n\nmore prose\n";
        let chunks = split(body);
        assert!(
            chunks.iter().all(|c| c.heading_path == "Real"),
            "a comment in a fence opened a section: {:?}",
            chunks.iter().map(|c| &c.heading_path).collect::<Vec<_>>()
        );
    }

    #[test]
    fn a_hash_without_a_space_is_not_a_heading() {
        assert_eq!(split("#hashtag and prose")[0].heading_path, "");
        assert_eq!(split("####### seven hashes")[0].heading_path, "");
    }

    #[test]
    fn text_before_the_first_heading_keeps_an_empty_path() {
        let chunks = split("Preamble.\n\n# Later\n\nAfter.\n");
        assert_eq!(chunks[0].heading_path, "");
        assert_eq!(chunks[0].text, "Preamble.");
        assert_eq!(chunks[1].heading_path, "Later");
    }

    #[test]
    fn a_long_section_is_wrapped_and_every_passage_fits() {
        let para = "Sentence about storage and retrieval. ".repeat(200);
        let chunks = split(&para);
        assert!(chunks.len() > 1, "expected a wrap, got {}", chunks.len());
        for c in &chunks {
            assert!(
                c.text.len() <= MAX_CHARS,
                "passage of {} exceeds {MAX_CHARS}",
                c.text.len()
            );
        }
    }

    /// The point of the overlap: nothing falls down the gap between passages.
    #[test]
    fn consecutive_passages_overlap_rather_than_abut() {
        let para = "alpha beta gamma delta epsilon zeta ".repeat(120);
        let chunks = split(&para);
        assert!(chunks.len() > 1);
        for pair in chunks.windows(2) {
            assert!(
                pair[1].start < pair[0].end,
                "passages {} and {} do not overlap",
                pair[0].ordinal,
                pair[1].ordinal
            );
        }
    }

    /// Offsets have to point back at the text the passage actually holds.
    #[test]
    fn offsets_index_the_body_they_came_from() {
        let body = "# Heading\n\nSome prose here.\n\n## Next\n\nMore prose.\n";
        for c in split(body) {
            assert_eq!(
                &body[c.start..c.end],
                c.text,
                "offsets do not recover the passage"
            );
        }
    }

    /// The panic this avoids arrives from whichever document first has an
    /// em-dash near a wrap, which on this corpus is most of them.
    #[test]
    fn multibyte_characters_near_a_wrap_do_not_panic() {
        let body = "— a paragraph with §, › and — throughout it all. ".repeat(120);
        let chunks = split(&body);
        assert!(chunks.len() > 1);
        for c in &chunks {
            assert_eq!(&body[c.start..c.end], c.text);
        }
    }

    #[test]
    fn ordinals_run_from_zero_without_gaps() {
        let body = format!("# A\n\n{}\n\n# B\n\nshort\n", "long prose. ".repeat(300));
        let chunks = split(&body);
        let ordinals: Vec<usize> = chunks.iter().map(|c| c.ordinal).collect();
        assert_eq!(ordinals, (0..chunks.len()).collect::<Vec<_>>());
    }

    #[test]
    fn the_embedded_text_carries_the_title_and_the_path() {
        let chunks = split("# Storage\n\nOne SQLite file.\n");
        let text = chunks[0].embed_text("Keel — Technical Specification");
        assert!(text.starts_with("Keel — Technical Specification › Storage"));
        assert!(text.contains("One SQLite file."));
    }

    #[test]
    fn a_document_with_no_headings_still_gets_the_title() {
        let chunks = split("Plain prose with no headings.");
        let text = chunks[0].embed_text("A decision");
        assert!(text.starts_with("A decision\n\n"), "{text}");
    }

    /// A single unbroken run with no whitespace still has to terminate.
    #[test]
    fn a_run_with_nowhere_to_break_is_cut_anyway() {
        let body = "x".repeat(MAX_CHARS * 3);
        let chunks = split(&body);
        assert!(chunks.len() > 1);
        for c in &chunks {
            assert!(c.text.len() <= MAX_CHARS);
        }
    }
}
