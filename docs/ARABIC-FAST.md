# Arabic support on GPUI Fast

This fork uses the official GPUI Fast 0.1.6 runtime and platform crates. The
runtime is retained under `crates/fast-runtime` with its license and provenance.
The `arabic` feature adapts the Parley engine from the private
`aliallaoua/gpui-arabic` repository at revision
`ff9e23a5942e92ddcc5e1990ad64561f9285b2db` (branch
`fast-shaping-performance`). Building requires access to that repository and
Rust 1.95 or newer (pinned in this fork). Both Arabic dependencies use the same
Git revision, so consumers do not need sibling source checkouts.

The engine retains its history from `holool/gpui-arabic`. Its optimized revision
is published separately in a private repository because GitHub disables formal
forks of the original repository. No private engine source is included in this
public Kit repository.

## Application APIs

Use `gpui_kit::component` (the `gpui-component` crate) for Sidebar, Input and
Button. The example contains no direct Base imports. Component uses Base as
its internal editing engine; the editing fixes live there so Component Input
inherits visual movement, bidi affinity, hit testing and selection geometry.

Set the component locale with `component::set_locale("ar")`, place the sidebar
on `Side::Right`, and reverse application rows where appropriate. Sidebar
menu content mirrors Arabic/Persian/Hebrew/Urdu locales. Input alignment can
be set explicitly with `.text_right()`; shaping determines paragraph direction
from the text. Layout direction is not inherited automatically in Fast.

## Rendering and performance

Ordinary text delegates to Fast's original platform shaper. The additional
bidi predicate runs on a shaping miss. Arabic initializes Parley lazily, uses
Fast's existing line/recent-shape caches, and retains native caret geometry
with cached layouts. Wrapping has a four-entry per-line cache. Painting uses
Fast's batched glyph painter and glyph atlas; layout retention and scene
composition remain upstream. Arabic glyphs currently use Swash grayscale
rasterization. The default non-Fast backend is unchanged.

Lines that already fit reuse their native layout instead of shaping a second
copy for wrapping. Dead layout owners are reclaimed with a bounded sweep,
avoiding periodic scans of every live layout. Font descriptors are converted
once, on their first Arabic use. For lines with at least 64 glyphs, a batched
grapheme geometry lookup replaces repeated glyph hit tests where the source
range is unambiguous; controls, overlapping geometry and other ambiguous cases
retain native caret lookup. Caret and selection interaction always uses the
retained native engine.

The font conversion preserves Parley's canonical high-bit namespace. Clearing
that bit produces correct geometry but no glyph bitmaps, the cause of the
initial invisible-text regression. Native verification must include visible
rendering, not just successful shaping.

An October 9, 2026 comparison on Apple M1 Pro used bundled Noto Sans Arabic,
1,000 distinct order labels, seven fresh text systems per process, and three
alternating process pairs. The first line is timed separately; values below
are median uncached averages for the remaining 999 lines. Repeating the same
order label stresses longer mixed Arabic, Latin, number and diacritic text.

| Arabic label length | Original adapter at `4e003a4` | Optimized adapter |
| --- | ---: | ---: |
| One order label | 45.74 µs/line | 46.24 µs/line |
| Four copies | 159.50 µs/line | 159.85 µs/line |
| Sixteen copies | 704.19 µs/line | 583.36 µs/line |

The long-line case is 17.2% faster; short-line shaping is effectively unchanged.
English uncached medians differ by less than 1% across these text lengths.
These are text microbenchmarks, not frame-rate results or a guarantee of zero
overhead. The small-line cutoff avoids batching overhead on ordinary labels.
Repeat on target hardware before application performance claims.

## Further shaping optimization

The engine reuses up to eight exact, previously verified font-instance
requests, skips color-table scans for faces already known to have no color
artwork, and uses binary search for grapheme boundaries. Fast disables the
engine's final-result cache because Fast already owns those layouts. The
shaped-paragraph cache remains enabled so wrapping reuses shaping; other engine
callers retain their existing cache defaults.

A second controlled comparison uses `1bb0f74` as its baseline and the same
release fixture, fonts, 1,000 labels, seven fresh text systems, and three
alternating process pairs per length:

| Arabic label length | Before µs/line | After µs/line | Time reduction |
| --- | ---: | ---: | ---: |
| One order label | 45.91 | 37.63 | 18.0% |
| Four copies | 158.68 | 134.37 | 15.3% |
| Sixteen copies | 578.36 | 492.96 | 14.8% |

The short-label Arabic-to-English ratio falls from 3.77× to 3.05×. English
medians differ by −0.5% to +1.2%. All nine paired Arabic comparisons improve.
These are uncached text-layout times; this iteration makes no new FPS claim.
The native engine passes 62 release tests and strict Clippy checks. Fast's
consumer harness continues to compare against the previous engine pin for all
216 cases; the signed Components example passes shaping, wrapping, mixed
selection and visual Input arrow checks. Arabic text and diacritics remain
visible in the native example.

## Reproduce

The examples are standalone workspaces, independent of Kit's optional shell
and its QuickJS Git dependencies:

```sh
./script/run-arabic-fast-macos --self-test
cargo test --release --manifest-path examples/arabic-performance/Cargo.toml \
  --features arabic --test arabic-regressions
cargo build --release --manifest-path examples/arabic-performance/Cargo.toml
# Save that baseline executable before the feature build replaces it.
cargo build --release --manifest-path examples/arabic-performance/Cargo.toml --features arabic
```

Run the demo from a signed macOS `.app` bundle for native UI checks; use
`--self-test` for automated shaping, mixed selection, wrapping, cache lifetime
and Input visual-arrow checks. The self-test exits nonzero on failure. Manual
visual verification confirms Arabic labels, Input text, mixed English/numbers
and diacritics are rendered. Native automation could read these values, but
its accessibility click did not focus the field; no manual keyboard test is
claimed.

The public-consumer regression harness runs on the macOS process main thread.
It checks glyph IDs, positions and UTF-8 source indices against the original
native caret model for 216 combinations of text, font, size and length. It also
checks exact caret positions after wrapping, live layout retention through
3,000 unique cache insertions, and eventual release of dead geometry. It does
not require the omitted upstream runtime unit-test fixtures.

The native example, isolated Kit Fast consumer and default GPUI consumer compile. Full workspace
validation and regeneration of the upstream workspace lockfile are pending:
the unrelated QuickJS Git fetch timed out. The standalone example lockfiles
resolve the patched Fast 0.1.6 graph. Upstream
runtime unit tests require image/font fixtures omitted from the published
Fast archive. Supplying those fixtures and enabling test support exposed an
existing property-test macro compilation error in `randomized_element_tree.rs`;
the two new boundary unit tests could not execute. Native checks exercise
canonical font metrics across the same renderer boundary. Other platforms and end-to-end retained-frame timing are unverified.

## Scope limits

Fixed character-cell repositioning is skipped for Arabic to preserve cursive
joins and caret geometry. Inline token rows and splitting a shaped line are
not yet validated for mixed bidi content. Swash grayscale rendering can differ
from native CoreText rasterization. This branch is experimental until those
cases and target-platform performance have been verified.
