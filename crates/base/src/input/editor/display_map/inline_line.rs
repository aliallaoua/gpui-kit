//! A visual row whose offsets remain in source UTF-8 bytes.
use gpui::{App, Pixels, Point, ShapedLine, SharedString, TextAlign, Window, point, px};
use std::ops::Range;

pub(crate) struct InlineFragment {
    pub(crate) range: Range<usize>,
    pub(crate) x: Pixels,
    pub(crate) width: Pixels,
    pub(crate) text: Option<ShapedLine>,
}

pub(crate) struct InputLine {
    pub(crate) len: usize,
    pub(crate) width: Pixels,
    pub(crate) text: SharedString,
    content: Content,
    #[cfg(feature = "gpui-fast")]
    geometry: Option<std::sync::Arc<gpui::arabic::ArabicLayout>>,
}
// Keep the ordinary shaped row inline: boxing it would add an allocation to
// every existing plain-text row. Only token rows allocate fragment storage.
#[allow(clippy::large_enum_variant)]
enum Content {
    Text(ShapedLine),
    Inline(Vec<InlineFragment>),
}
impl From<ShapedLine> for InputLine {
    fn from(line: ShapedLine) -> Self {
        Self {
            len: line.len,
            width: line.width,
            text: line.text.clone(),
            #[cfg(feature = "gpui-fast")]
            geometry: gpui::arabic::layout_for(&line),
            content: Content::Text(line),
        }
    }
}
impl InputLine {
    pub(crate) fn inline(text: SharedString, fragments: Vec<InlineFragment>) -> Self {
        let width = fragments.last().map_or(px(0.), |f| f.x + f.width);
        Self {
            len: text.len(),
            text,
            width,
            content: Content::Inline(fragments),
            #[cfg(feature = "gpui-fast")]
            geometry: None,
        }
    }
    #[cfg(feature = "gpui-fast")]
    pub(crate) fn x_for_index_with_affinity(&self, ix: usize, upstream: bool) -> Pixels {
        #[cfg(feature = "gpui-fast")]
        if let Some(geometry) = &self.geometry {
            return geometry
                .caret_bounds(Self::caret(ix, upstream), px(1.))
                .map_or(px(0.), |bounds| bounds.origin.x);
        }
        let _ = upstream;
        self.x_for_index(ix)
    }
    #[cfg(feature = "gpui-fast")]
    fn caret(index: usize, upstream: bool) -> gpui::arabic::CaretPosition {
        gpui::arabic::CaretPosition {
            index,
            affinity: if upstream {
                gpui::arabic::CaretAffinity::Upstream
            } else {
                gpui::arabic::CaretAffinity::Downstream
            },
        }
    }
    #[cfg(feature = "gpui-fast")]
    pub(crate) fn closest_caret_for_x(&self, x: Pixels) -> (usize, bool) {
        #[cfg(feature = "gpui-fast")]
        if let Some(geometry) = &self.geometry {
            let caret = geometry
                .caret_for_point(point(x, px(0.5)), px(1.))
                .unwrap_or_else(|caret| caret);
            return (
                caret.index,
                caret.affinity == gpui::arabic::CaretAffinity::Upstream,
            );
        }
        (self.closest_index_for_x(x), false)
    }
    #[cfg(feature = "gpui-fast")]
    pub(crate) fn adjacent_visual_caret(
        &self,
        index: usize,
        upstream: bool,
        right: bool,
    ) -> Option<(usize, bool)> {
        #[cfg(feature = "gpui-fast")]
        if let Some(geometry) = &self.geometry {
            let next = geometry.adjacent_visual_caret(
                Self::caret(index, upstream),
                if right {
                    gpui::arabic::VisualDirection::Right
                } else {
                    gpui::arabic::VisualDirection::Left
                },
            )?;
            return Some((
                next.index,
                next.affinity == gpui::arabic::CaretAffinity::Upstream,
            ));
        }
        let _ = (index, upstream, right);
        None
    }
    #[cfg(feature = "gpui-fast")]
    pub(crate) fn word_caret(
        &self,
        index: usize,
        upstream: bool,
        right: bool,
    ) -> Option<(usize, bool)> {
        #[cfg(feature = "gpui-fast")]
        if let Some(geometry) = &self.geometry {
            let caret = Self::caret(index, upstream);
            let next = geometry.word_caret(caret, right);
            if next == caret {
                return None;
            }
            return Some((
                next.index,
                next.affinity == gpui::arabic::CaretAffinity::Upstream,
            ));
        }
        let _ = (index, upstream, right);
        None
    }
    #[cfg(feature = "gpui-fast")]
    pub(crate) fn is_rtl(&self) -> bool {
        #[cfg(feature = "gpui-fast")]
        if let Some(geometry) = &self.geometry {
            return geometry.is_rtl();
        }
        false
    }
    #[cfg(feature = "gpui-fast")]
    pub(crate) fn selection_bounds(
        &self,
        range: Range<usize>,
        height: Pixels,
    ) -> smallvec::SmallVec<[gpui::Bounds<Pixels>; 4]> {
        #[cfg(feature = "gpui-fast")]
        if let Some(geometry) = &self.geometry {
            return geometry
                .selection_bounds(range, height)
                .into_iter()
                .collect();
        }
        let left = self.x_for_index(range.start);
        let right = self.x_for_index(range.end);
        smallvec::smallvec![gpui::Bounds::new(
            point(left, px(0.)),
            gpui::size(right - left, height),
        )]
    }
    pub(crate) fn x_for_index(&self, ix: usize) -> Pixels {
        match &self.content {
            Content::Text(line) => line.x_for_index(ix),
            Content::Inline(fragments) => {
                for f in fragments {
                    if ix < f.range.end {
                        return f.x
                            + f.text.as_ref().map_or(
                                if ix <= f.range.start { px(0.) } else { f.width },
                                |line| line.x_for_index(ix.saturating_sub(f.range.start)),
                            );
                    }
                }
                self.width
            }
        }
    }
    pub(crate) fn closest_index_for_x(&self, x: Pixels) -> usize {
        match &self.content {
            Content::Text(line) => line.closest_index_for_x(x),
            Content::Inline(fragments) => {
                for f in fragments {
                    if x <= f.x + f.width {
                        return f.text.as_ref().map_or(
                            if x - f.x < f.width / 2. {
                                f.range.start
                            } else {
                                f.range.end
                            },
                            |line| f.range.start + line.closest_index_for_x(x - f.x),
                        );
                    }
                }
                self.len
            }
        }
    }
    pub(crate) fn index_for_x(&self, x: Pixels) -> Option<usize> {
        match &self.content {
            Content::Text(line) => line.index_for_x(x),
            Content::Inline(_) => {
                (x >= px(0.) && x <= self.width).then(|| self.closest_index_for_x(x))
            }
        }
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn paint(
        &self,
        pos: Point<Pixels>,
        height: Pixels,
        align: TextAlign,
        width: Option<Pixels>,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.paint_pass(pos, height, align, width, false, window, cx);
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn paint_background(
        &self,
        pos: Point<Pixels>,
        height: Pixels,
        align: TextAlign,
        width: Option<Pixels>,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.paint_pass(pos, height, align, width, true, window, cx);
    }
    #[allow(clippy::too_many_arguments)]
    fn paint_pass(
        &self,
        pos: Point<Pixels>,
        height: Pixels,
        align: TextAlign,
        width: Option<Pixels>,
        background: bool,
        window: &mut Window,
        cx: &mut App,
    ) {
        let paint = |line: &ShapedLine, pos, align, width, window: &mut Window, cx: &mut App| {
            if background {
                let _ = line.paint_background(pos, height, align, width, window, cx);
            } else {
                let _ = line.paint(pos, height, align, width, window, cx);
            }
        };
        match &self.content {
            Content::Text(line) => paint(line, pos, align, width, window, cx),
            Content::Inline(fragments) => {
                let remaining = (width.unwrap_or(self.width) - self.width).max(px(0.));
                let offset = match align {
                    TextAlign::Right => remaining,
                    TextAlign::Center => remaining / 2.,
                    _ => px(0.),
                };
                for f in fragments {
                    if let Some(line) = &f.text {
                        paint(
                            line,
                            pos + point(offset + f.x, px(0.)),
                            TextAlign::Left,
                            None,
                            window,
                            cx,
                        );
                    }
                }
            }
        }
    }
}
