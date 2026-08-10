/**
 * What jsdom does not have.
 *
 * `scrollIntoView` is unimplemented there. Guarding every call site for the
 * benefit of the test environment would put test scaffolding into product code,
 * so the environment is patched instead — the calls stay honest and a real
 * browser is unaffected.
 */

if (!Element.prototype.scrollIntoView) {
  Element.prototype.scrollIntoView = function scrollIntoView() {};
}
