# Arabic support on GPUI Fast

This fork uses the official GPUI Fast 0.1.6 runtime and platform crates. The
runtime is retained under `crates/fast-runtime` with its license and provenance.
The `arabic` feature adapts the pinned Parley engine from `holool/gpui-arabic`
revision `18a98efc902cdb1d92fbb764f690818ffbee865e`; it does not copy that private
engine's source into this public fork. Building requires access to that repo and Rust 1.95 or newer (pinned in this fork).

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

The font conversion preserves Parley's canonical high-bit namespace. Clearing
that bit produces correct geometry but no glyph bitmaps, the cause of the
initial invisible-text regression. Native verification must include visible
rendering, not just successful shaping.

The first optimized macOS text-layout measurement used 1,000 distinct order
labels, seven samples, and 100 cache passes per sample:

| Case | Fast baseline | Arabic port |
| --- | ---: | ---: |
| ASCII uncached, median | 19.63 µs/line | 19.19 µs/line |
| ASCII cached, median | 77 ns/line | 80 ns/line |
| Arabic uncached, median | — | 59.81 µs/line |
| Arabic cached, median | — | 83 ns/line |

These are text-layout microbenchmarks, not frame-rate results or a guarantee
of zero overhead. The few-nanosecond cached difference is within the run's
variation. Arabic uncached samples include lazy font catalog initialization
amortized over 1,000 lines. Repeat on target hardware before performance claims.

## Reproduce

The examples are standalone workspaces, independent of Kit's optional shell
and its QuickJS Git dependencies:

```sh
./script/run-arabic-fast-macos --self-test
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
