//! Public-consumer checks: the runtime dependency is built normally, avoiding
//! unrelated upstream GPUI test fixtures while exercising the real adapter.
use arabic_core as ce;
use ce::PlatformTextSystem as _;
use gpui::{LineLayout, Platform as _, TextRun, TextSystem, WindowTextSystem, black, font, px};
use std::sync::Arc;

fn system() -> WindowTextSystem {
    let platform = gpui_macos::MacPlatform::new(true);
    WindowTextSystem::new(Arc::new(TextSystem::new(platform.text_system())))
}

fn run(len: usize) -> TextRun {
    TextRun {
        len,
        font: font("Arial"),
        color: black(),
        background_color: None,
        underline: None,
        strikethrough: None,
    }
}

fn shape(system: &WindowTextSystem, text: &str) -> Arc<LineLayout> {
    system.layout_line(text, px(18.), &[run(text.len())], None)
}

fn native_system() -> arabic_parley::ParleyTextSystem {
    arabic_parley::ParleyTextSystem::new_with_system_font(
        arabic_parley::SystemFonts::Load,
        ".AppleSystemUIFont",
    )
    .with_automatic_optical_sizing()
    .with_fallback_families(["Noto Sans Arabic", "Arial", "Noto Sans", "DejaVu Sans"])
}

fn glyph_source_mapping_matches_native_caret_model() {
    let system = system();
    let native = native_system();
    let examples = [
        "سلام",
        "مَرْحَبًا أحمد، الإجمالي ١٬٢٥٠ د.ج",
        "abc سلام def — Order #123",
        "لا إله إلا الله — office ffi",
        "س\u{200d}لام س\u{200c}لام",
        "\u{200f}طلب 123\u{200e}",
        "abc\u{2067}سلام 123\u{2069}def",
        "العربية עם עברית English 123",
        "طلب 👩‍👩‍👧‍👦 مَرْحَبًا 🙂",
        "\u{064e}\u{0651}مَرْحَبًا ـــ طلب (١٢٣) [abc]",
        "\u{061c}الإجمالي ١٬٢٥٠ د.ج\tجاهز",
        "طلب\nمَرْحَبًا",
    ];
    for family in ["Arial", ".AppleSystemUIFont"] {
        for size in [12., 18., 32.] {
            for repeats in [1, 4, 16] {
                for example in examples {
                    let text = example.repeat(repeats);
                    let mut text_run = run(text.len());
                    text_run.font = font(family);
                    let fast = system.layout_line(&text, px(size), &[text_run], None);
                    let expected = native.layout_text(ce::TextLayoutRequest {
                        text: &text,
                        font_size: ce::px(size),
                        runs: &[ce::TextRun {
                            len: text.len(),
                            font: ce::font(family),
                            color: ce::black(),
                            background_color: None,
                            underline: None,
                            strikethrough: None,
                            letter_spacing: None,
                        }],
                        options: ce::TextLayoutOptions {
                            text_align: ce::TextAlign::Left,
                            ..Default::default()
                        },
                    });
                    assert_eq!(fast.runs.len(), expected.paint_fragments.len(), "{text}");
                    for (run, fragment) in fast.runs.iter().zip(&expected.paint_fragments) {
                        assert_eq!(run.glyphs.len(), fragment.glyphs.len(), "{text}");
                        for (actual, glyph) in run.glyphs.iter().zip(fragment.glyphs.iter()) {
                            let caret = expected
                                .platform_layout
                                .caret_from_pixel_point(
                                    ce::point(glyph.position.x + ce::px(0.1), ce::px(size / 2.)),
                                    ce::px(size),
                                )
                                .unwrap_or_else(|c| c);
                            let range = if caret.affinity == ce::CaretAffinity::Upstream {
                                expected.platform_layout.logical_cluster_before(caret)
                            } else {
                                expected.platform_layout.logical_cluster_after(caret)
                            };
                            assert_eq!(
                                actual.index,
                                range.map_or(caret.index, |r| r.start),
                                "{text}"
                            );
                            assert!(text.is_char_boundary(actual.index));
                            assert_eq!(actual.id.0, glyph.id.0);
                            assert!(
                                (f32::from(actual.position.x) - f32::from(glyph.position.x)).abs()
                                    < 0.001
                            );
                        }
                    }
                }
            }
        }
    }
}

fn cache_churn_preserves_live_geometry_and_releases_dead_owners() {
    let dead = {
        let system = system();
        let line = shape(&system, "هذا التخطيط يجب تحريره بعد إسقاط مالكه");
        Arc::downgrade(&gpui::arabic::layout_for(&line).unwrap())
    };
    let system = system();
    let live = shape(&system, "هذا التخطيط يبقى صالحًا ما دام مالكه موجودًا");
    let live_geometry = Arc::downgrade(&gpui::arabic::layout_for(&live).unwrap());
    for index in 0..3000 {
        let _ = shape(&system, &format!("طلب رقم {index} جاهز للاستلام"));
    }
    assert!(
        dead.upgrade().is_none(),
        "dead native geometry was retained"
    );
    assert!(live_geometry.upgrade().is_some());
    assert!(gpui::arabic::layout_for(&live).is_some());
    assert!(live.x_for_index(0) > live.x_for_index(live.len));
}

fn fitting_and_wrapped_arabic_keep_native_caret_geometry() {
    let system = system();
    let native = native_system();
    let text = "مَرْحَبًا بكم في سديد — Order 123 جاهز للاستلام";
    for (width, expected_wraps) in [(px(2000.), false), (px(100.), true)] {
        let lines = system
            .shape_text(
                text.to_owned().into(),
                px(18.),
                &[run(text.len())],
                Some(width),
                None,
            )
            .unwrap();
        assert_eq!(!lines[0].wrap_boundaries.is_empty(), expected_wraps);
        let expected = native.layout_text(ce::TextLayoutRequest {
            text,
            font_size: ce::px(18.),
            runs: &[ce::TextRun {
                len: text.len(),
                font: ce::font("Arial"),
                color: ce::black(),
                background_color: None,
                underline: None,
                strikethrough: None,
                letter_spacing: None,
            }],
            options: ce::TextLayoutOptions {
                text_align: ce::TextAlign::Left,
                wrap_width: Some(ce::px(f32::from(width))),
                ..Default::default()
            },
        });
        for index in text.char_indices().map(|(i, _)| i).chain([text.len()]) {
            let actual = lines[0].position_for_index(index, px(24.)).unwrap();
            let expected = expected
                .platform_layout
                .caret_bounds(
                    ce::CaretPosition::attached_to_next_cluster(index),
                    ce::px(24.),
                )
                .unwrap()
                .origin;
            assert!((f32::from(actual.x) - f32::from(expected.x)).abs() < 0.001);
            assert!((f32::from(actual.y) - f32::from(expected.y)).abs() < 0.001);
        }
    }
}

fn main() {
    // MacPlatform must be created on the process main thread; use a native
    // harness instead of libtest's worker threads.
    glyph_source_mapping_matches_native_caret_model();
    cache_churn_preserves_live_geometry_and_releases_dead_owners();
    fitting_and_wrapped_arabic_keep_native_caret_geometry();
    println!("3 Arabic regression checks passed");
}
