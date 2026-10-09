# GPUI Kit

One dependency for building desktop applications with GPUI:

```toml
[dependencies]
gpui-kit = "0.7"
```

This fork adds Arabic shaping and editing to the GPUI Fast 0.1.6 backend.
Use GPUI Component APIs through `gpui_kit::component` for application controls.

```toml
[dependencies]
gpui-kit = { git = "https://github.com/aliallaoua/gpui-kit", branch = "arabic-fast", features = ["gpui-fast"] }
[patch.crates-io]
gpui-fast = { git = "https://github.com/aliallaoua/gpui-kit", branch = "arabic-fast" }
```

The patch keeps the platform crates and Kit on the same patched runtime.
Pin both Git dependencies to the same revision for production. Building
requires access to the existing private `holool/gpui-arabic` text engine.
See the fork's [Arabic Fast notes](../../docs/ARABIC-FAST.md) for scope,
performance measurements, and validation.

`gpui-kit` depends on the matching set of GPUI crates, so an application
never lists GPUI itself. `use gpui_kit::*;` is GPUI, and each layer is
reachable by name:

| Path                  | Crate             | Feature          |
| --------------------- | ----------------- | ---------------- |
| `gpui_kit::*`         | `gpui`            | always           |
| `gpui_kit::platform`  | `gpui_platform`   | always           |
| `gpui_kit::base`      | `gpui-base`       | always           |
| `gpui_kit::component` | `gpui-component`  | `component` (on) |
| `gpui_kit::assets`    | `gpui-kit-assets` | `assets` (on)    |

`gpui_kit::application()` opens the platform and `gpui_kit::init()`
initializes the enabled layers:

```rust
use gpui_kit::component::button::*;
use gpui_kit::*;

struct Hello;

impl Render for Hello {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().child(Button::new("ok").primary().label("Let's Go!"))
    }
}

fn main() {
    gpui_kit::application().run(|cx| {
        gpui_kit::init(cx);
        gpui_kit::open_window(WindowOptions::default(), cx, |_, cx| cx.new(|_| Hello))
            .expect("failed to open window");
    });
}
```

The `gpui-component` features (`inspector`, `decimal`, `tree-sitter`,
`tree-sitter-languages`, and each `tree-sitter-<language>`) are available on
`gpui-kit` under the same names. `test-support` turns on GPUI's test harness
for `#[gpui_kit::test]`, `TestAppContext`, `VisualTestContext`, and native-platform
rendering support; enable it under `[dev-dependencies]`. It also exposes
`gpui_kit::test::{TestWindowExt, TestAppContextExt, TestSupportExt, ElementSnapshot}` for
UI integration testing: render real components in headless windows, simulate input,
and verify state, focus, layout and application callbacks. `gpui_kit::TestSupportExt` is also
available without the feature: `.test_support()` keeps production render chains
intact and returns the original element in normal builds. Snapshots read native
accessibility properties without hand-supplied state. See [UI integration testing](TESTING.md) for usage.
The independent `profiler` feature enables GPUI frame-event instrumentation and is off by default.
See <https://gpui-kit.com> for the guides.
