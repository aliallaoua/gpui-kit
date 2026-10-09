use gpui_kit::assets::Assets;
use gpui_kit::component::{
    ActiveTheme, IconName, Side,
    button::Button,
    input::{Input, InputState},
    sidebar::{
        Sidebar, SidebarFooter, SidebarGroup, SidebarHeader, SidebarMenu, SidebarMenuItem,
        SidebarToggleButton,
    },
};
use gpui_kit::prelude::*;
use gpui_kit::*;

struct Example {
    input: Entity<InputState>,
    collapsed: bool,
    rtl: bool,
    active: usize,
}
impl Example {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| {
            let mut state = InputState::new(window, cx).placeholder("اكتب اسم العميل");
            state.set_value("سلام", window, cx);
            state
        });
        Self {
            input,
            collapsed: false,
            rtl: true,
            active: 0,
        }
    }
}
impl Render for Example {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let tokens = theme.semantic_tokens();
        let side = if self.rtl { Side::Right } else { Side::Left };
        let labels = if self.rtl {
            ["نظرة عامة", "الطلبات", "العملاء", "الإعدادات"]
        } else {
            ["Overview", "Orders", "Customers", "Settings"]
        };
        let icons = [
            IconName::LayoutDashboard,
            IconName::Inbox,
            IconName::User,
            IconName::Settings,
        ];
        let menu =
            SidebarMenu::new().children(labels.into_iter().enumerate().map(|(index, label)| {
                SidebarMenuItem::new(label)
                    .icon(icons[index].clone())
                    .active(self.active == index)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.active = index;
                        cx.notify();
                    }))
            }));
        div()
            .id("arabic-showcase")
            .size_full()
            .flex()
            .flex_row()
            .when(self.rtl, |this| this.flex_row_reverse())
            .bg(tokens.colors.background)
            .text_color(tokens.colors.foreground)
            .font_family(".SystemUIFont")
            .child(
                Sidebar::new("arabic-sidebar")
                    .side(side)
                    .collapsed(self.collapsed)
                    .header(SidebarHeader::new().child(if self.collapsed {
                        "س"
                    } else {
                        "سديد"
                    }))
                    .child(
                        SidebarGroup::new(if self.rtl {
                            "إدارة المطعم"
                        } else {
                            "Restaurant"
                        })
                        .child(menu),
                    )
                    .footer(
                        SidebarFooter::new().child(
                            SidebarToggleButton::new()
                                .side(side)
                                .collapsed(self.collapsed)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.collapsed = !this.collapsed;
                                    cx.notify();
                                })),
                        ),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .h_full()
                    .flex_1()
                    .min_w_0()
                    .p_8()
                    .gap_6()
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .justify_between()
                            .child(div().text_2xl().child(labels[self.active]))
                            .child(
                                Button::new("language")
                                    .label(if self.rtl {
                                        "English"
                                    } else {
                                        "العربية"
                                    })
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.rtl = !this.rtl;
                                        gpui_kit::component::set_locale(if this.rtl {
                                            "ar"
                                        } else {
                                            "en"
                                        });
                                        cx.notify();
                                    })),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_3()
                            .child("اسم العميل · Customer name")
                            .child(
                                Input::new(&self.input).when(self.rtl, |this| this.text_right()),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_3()
                            .p_6()
                            .rounded(tokens.radius.lg)
                            .bg(tokens.colors.surface)
                            .border_1()
                            .border_color(tokens.colors.border)
                            .child("طلب #A123 — جاهز للاستلام")
                            .child("الإجمالي: ١٬٢٥٠ د.ج · Total: 1,250 DZD")
                            .child("العربية مع English، الأرقام 123، والتشكيل: مَرْحَبًا"),
                    ),
            )
    }
}
fn check_shaping(window: &Window) -> Result<(), String> {
    use gpui_kit::arabic::{self, CaretPosition, VisualDirection};
    let text_system = window.text_system();
    let shape = |text: &str| {
        text_system.shape_line(
            text.to_owned().into(),
            px(18.),
            &[TextRun {
                len: text.len(),
                font: font(".AppleSystemUIFont"),
                color: black(),
                background_color: None,
                underline: None,
                strikethrough: None,
            }],
            None,
        )
    };
    let latin = shape("Order 123");
    if arabic::layout_for(&latin).is_some() {
        return Err("Latin should use the original platform engine".into());
    }
    for text in ["سلام", "مَرْحَبًا", "abc سلام def", "طلب #A123 — ١٬٢٥٠ د.ج"]
    {
        let line = shape(text);
        let geometry =
            arabic::layout_for(&line).ok_or_else(|| format!("missing geometry: {text}"))?;
        for run in &line.runs {
            // Exercise canonical font lookup across the renderer boundary,
            // not only geometry (which succeeds even with a broken FontId).
            let metrics = text_system.bounding_box(run.font_id, px(18.));
            if metrics.size.height <= px(0.) {
                return Err("empty Arabic font metrics".into());
            }
        }
        let mut caret = geometry
            .caret_for_point(point(px(-10.), px(12.)), px(24.))
            .unwrap_or_else(|c| c);
        let mut previous_x = px(-10.);
        let mut steps = 0;
        while let Some(next) = geometry.adjacent_visual_caret(caret, VisualDirection::Right) {
            if next == caret {
                break;
            }
            if !text.is_char_boundary(next.index) {
                return Err("caret split a UTF-8 character".into());
            }
            let x = geometry
                .caret_bounds(next, px(24.))
                .ok_or("missing caret bounds")?
                .origin
                .x;
            if x + px(0.1) < previous_x {
                return Err(format!("visual right moved left in {text}"));
            }
            previous_x = x;
            caret = next;
            steps += 1;
            if steps > text.len() * 2 {
                return Err("visual caret movement did not terminate".into());
            }
        }
        if steps == 0 {
            return Err(format!("no visual caret movement in {text}"));
        }
    }
    let retained = shape("سلام");
    for index in 0..1200 {
        let _ = shape(&format!("طلب رقم {index}"));
    }
    if arabic::layout_for(&retained).is_none() {
        return Err("live Arabic geometry was evicted".into());
    }
    let rtl = shape("سلام");
    if rtl.x_for_index(0) <= rtl.x_for_index("سلام".len()) {
        return Err("RTL source endpoints are reversed".into());
    }
    let mixed = shape("abc سلام def");
    let rectangles = arabic::layout_for(&mixed)
        .unwrap()
        .selection_bounds(0..6, px(24.));
    if rectangles.len() < 2 {
        return Err("mixed selection needs disjoint visual rectangles".into());
    }
    let text = "مَرْحَبًا بكم في سديد — Order 123 جاهز للاستلام";
    let wrapped = text_system
        .shape_text(
            text.to_owned().into(),
            px(18.),
            &[TextRun {
                len: text.len(),
                font: font(".AppleSystemUIFont"),
                color: black(),
                background_color: None,
                underline: None,
                strikethrough: None,
            }],
            Some(px(120.)),
            None,
        )
        .map_err(|e| e.to_string())?;
    if wrapped[0].wrap_boundaries.is_empty() {
        return Err("Arabic paragraph did not wrap".into());
    }
    for index in text
        .char_indices()
        .map(|(index, _)| index)
        .chain([text.len()])
    {
        if wrapped[0].position_for_index(index, px(24.)).is_none() {
            return Err(format!("missing wrapped caret at {index}"));
        }
    }
    let _ = CaretPosition::attached_to_next_cluster(0);
    println!("Arabic shaping, bidi hit testing, diacritics, mixed selection, wrapping: PASS");
    Ok(())
}
fn main() {
    let self_test = std::env::args().any(|arg| arg == "--self-test");
    let passed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let result = passed.clone();
    gpui_kit::application().with_assets(Assets).run(move |cx| {
        gpui_kit::init(cx);
        gpui_kit::component::set_locale("ar");
        let options = WindowOptions {
            titlebar: Some(TitlebarOptions {
                title: Some("GPUI Kit Fast Arabic".into()),
                ..Default::default()
            }),
            window_bounds: Some(WindowBounds::centered(size(px(1000.), px(640.)), cx)),
            ..Default::default()
        };
        let (_, example) = gpui_kit::open_window(options, cx, |window, cx| {
            cx.new(|cx| Example::new(window, cx))
        })
        .expect("open Arabic showcase");
        if self_test {
            let window = cx.windows()[0];
            window
                .update(cx, |_, window, cx| {
                    if let Err(error) = check_shaping(window) {
                        eprintln!("FAIL: {error}");
                        cx.quit();
                        return;
                    }
                    let input = example.read(cx).input.clone();
                    input.update(cx, |state, cx| {
                        state.set_selected_range(0..0, cx);
                        state.focus(window, cx);
                    });
                    // Actions are deferred by GPUI. Wait for the first painted frame,
                    // then inspect each result after its dispatch has been processed.
                    window.on_next_frame(move |window, _| {
                        window.on_next_frame(move |window, cx| {
                            let handle = window.window_handle();
                            window.dispatch_action(
                                Box::new(gpui_kit::component::input::MoveLeft),
                                cx,
                            );
                            cx.defer(move |cx| {
                                let cursor = input.read(cx).cursor();
                                if cursor == 0 || !"سلام".is_char_boundary(cursor) {
                                    eprintln!("FAIL: Arabic visual Left, cursor={cursor}");
                                    cx.quit();
                                    return;
                                }
                                handle
                                    .update(cx, move |_, window, cx| {
                                        window.dispatch_action(
                                            Box::new(gpui_kit::component::input::MoveRight),
                                            cx,
                                        );
                                        cx.defer(move |cx| {
                                            if input.read(cx).cursor() == 0 {
                                                result.store(
                                                    true,
                                                    std::sync::atomic::Ordering::Relaxed,
                                                );
                                                println!("Arabic Input visual movement: PASS");
                                            } else {
                                                eprintln!("FAIL: Arabic visual Right");
                                            }
                                            cx.quit();
                                        });
                                    })
                                    .expect("move right");
                            });
                        });
                    });
                })
                .expect("schedule Arabic smoke checks");
        }
        cx.activate(true);
    });
    if self_test && !passed.load(std::sync::atomic::Ordering::Relaxed) {
        std::process::exit(1);
    }
}
