use gpui::*;
use std::{hint::black_box, sync::Arc, time::Instant};
fn run(label: &str, input: &[String], warm: bool) {
    let platform = gpui_macos::MacPlatform::new(true);
    let system = WindowTextSystem::new(Arc::new(TextSystem::new(platform.text_system())));
    let font = font(".AppleSystemUIFont");
    let runs: Vec<_> = input
        .iter()
        .map(|s| {
            vec![TextRun {
                len: s.len(),
                font: font.clone(),
                color: black(),
                background_color: None,
                underline: None,
                strikethrough: None,
            }]
        })
        .collect();
    if warm {
        for (text, runs) in input.iter().zip(&runs) {
            black_box(system.layout_line(text, px(16.), runs, None));
        }
    }
    let start = Instant::now();
    for _ in 0..if warm { 100 } else { 1 } {
        for (text, runs) in input.iter().zip(&runs) {
            black_box(system.layout_line(text, px(16.), runs, None));
        }
    }
    let operations = input.len() * if warm { 100 } else { 1 };
    println!(
        "{label},{},{:.1}",
        if cfg!(feature = "arabic") {
            "arabic"
        } else {
            "baseline"
        },
        start.elapsed().as_nanos() as f64 / operations as f64
    );
}
fn main() {
    let ascii: Vec<_> = (0..1000)
        .map(|i| format!("Order #{i}: customer Jane Doe, total 1,250 DZD"))
        .collect();
    let arabic: Vec<_> = (0..1000)
        .map(|i| format!("طلب #{i} — مَرْحَبًا أحمد، الإجمالي ١٬٢٥٠ د.ج"))
        .collect();
    println!("case,backend,ns_per_line");
    for _ in 0..7 {
        run("ascii_cold", &ascii, false);
        run("ascii_cached", &ascii, true);
        if cfg!(feature = "arabic") {
            run("arabic_cold", &arabic, false);
            run("arabic_cached", &arabic, true);
        }
    }
}
