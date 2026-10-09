//! Optional Arabic shaping for GPUI Fast. Rendering retention stays in `fast/`.
//!
//! Ordinary text delegates to the original platform system. The Parley engine
//! and its font catalog are constructed only on the first bidirectional line.
use crate::*;
use arabic_core as ce;
use ce::PlatformTextSystem as _;
use parking_lot::Mutex;
use std::{
    borrow::Cow,
    collections::{HashMap, VecDeque},
    sync::{Arc, LazyLock, OnceLock, Weak},
};
use unicode_segmentation::UnicodeSegmentation as _;

pub use ce::{
    CaretAffinity, CaretPosition, TextBoundary, TextDirection, TextMovement, VisualDirection,
};
const FONT_TAG: usize = 1 << (usize::BITS - 1);
const CLEANUP_BUDGET: usize = 4;
const PENDING_CAPACITY: usize = 128;

/// Whether a line needs the bidi engine. The ASCII check is vectorized by Rust.
pub fn needs_bidi(text: &str) -> bool {
    !text.is_ascii()
        && text.chars().any(|c| {
            matches!(c as u32,
        0x0590..=0x08ff | 0x200e..=0x200f | 0x202a..=0x202e |
        0x2066..=0x2069 | 0xfb1d..=0xfdff | 0xfe70..=0xfeff | 0x1ee00..=0x1eeff)
        })
}
fn tagged(id: FontId) -> bool {
    id.0 & FONT_TAG != 0
}
// Parley's canonical font IDs already carry this namespace bit. Preserve it
// when returning to Parley: clearing it makes every raster lookup fail.
fn native_id(id: FontId) -> ce::FontId {
    ce::FontId(id.0)
}
fn fast_id(id: ce::FontId) -> FontId {
    FontId(id.0 | FONT_TAG)
}
fn cp(p: Pixels) -> ce::Pixels {
    ce::px(f32::from(p))
}
fn fp(p: ce::Pixels) -> Pixels {
    px(f32::from(p))
}
fn bounds(b: ce::Bounds<ce::Pixels>) -> Bounds<Pixels> {
    Bounds::new(
        point(fp(b.origin.x), fp(b.origin.y)),
        size(fp(b.size.width), fp(b.size.height)),
    )
}
fn descriptor(f: &Font) -> ce::Font {
    ce::Font {
        family: f.family.to_string().into(),
        features: ce::FontFeatures(f.features.0.clone()),
        fallbacks: f.fallbacks.as_ref().map(|f| ce::FontFallbacks(f.0.clone())),
        weight: ce::FontWeight(f.weight.0),
        style: match f.style {
            FontStyle::Normal => ce::FontStyle::Normal,
            FontStyle::Italic => ce::FontStyle::Italic,
            FontStyle::Oblique => ce::FontStyle::Oblique,
        },
    }
}

struct GlyphCluster {
    left: ce::Pixels,
    right: ce::Pixels,
    following_left: ce::Pixels,
    index: usize,
}

/// Resolve ordinary glyph positions with one backend cluster walk. The
/// compatibility representation needs byte indices, but interaction continues
/// to use the retained native layout and its caret affinity.
fn glyph_clusters(text: &str, layout: &ce::LineLayout) -> Option<Vec<GlyphCluster>> {
    if layout.visual_lines.len() != 1
        || layout
            .paint_fragments
            .iter()
            .map(|r| r.glyphs.len())
            .sum::<usize>()
            < 64
    {
        return None;
    }
    let ranges: Vec<_> = text
        .grapheme_indices(true)
        .map(|(start, grapheme)| start..start + grapheme.len())
        .collect();
    let geometry = layout.platform_layout.inline_geometry_for_ranges(&ranges);
    let mut clusters = Vec::with_capacity(ranges.len());
    for (range, rectangles) in ranges.into_iter().zip(geometry) {
        // Zero-width controls and disjoint graphemes need native hit testing;
        // avoid changing their boundary semantics to speed up ordinary labels.
        if rectangles.len() != 1 {
            return None;
        }
        let bounds = rectangles[0].bounds;
        clusters.push(GlyphCluster {
            left: bounds.left(),
            right: bounds.right(),
            following_left: ce::px(f32::MAX),
            index: range.start,
        });
    }
    clusters.sort_unstable_by(|a, b| f32::from(a.right).total_cmp(&f32::from(b.right)));
    let mut following_left = ce::px(f32::MAX);
    for cluster in clusters.iter_mut().rev() {
        // End-order does not imply start-order: a later cluster may contain
        // several earlier ones. Only use a byte index if no later box overlaps.
        cluster.following_left = following_left;
        if cluster.left < following_left {
            following_left = cluster.left;
        }
    }
    Some(clusters)
}

fn glyph_index(
    layout: &ce::LineLayout,
    clusters: Option<&[GlyphCluster]>,
    glyph: &ce::ShapedGlyph,
    height: ce::Pixels,
) -> usize {
    let x = glyph.position.x + ce::px(0.1);
    if let Some(clusters) = clusters {
        let i = clusters.partition_point(|cluster| cluster.right < x);
        if let Some(cluster) = clusters.get(i)
            && x >= cluster.left
            && x < cluster.following_left
        {
            return cluster.index;
        }
    }
    let caret = layout
        .platform_layout
        .caret_from_pixel_point(ce::point(x, height / 2.), height)
        .unwrap_or_else(|c| c);
    let range = if caret.affinity == ce::CaretAffinity::Upstream {
        layout.platform_layout.logical_cluster_before(caret)
    } else {
        layout.platform_layout.logical_cluster_after(caret)
    };
    range.map_or(caret.index, |r| r.start)
}

/// A line shaped by the Arabic runtime, retaining source UTF-8 offsets.
pub struct ArabicLayout {
    engine: Arc<arabic_parley::ParleyTextSystem>,
    text: Arc<str>,
    runs: Vec<ce::TextRun>,
    source_starts: Vec<usize>,
    pub(crate) layout: ce::LineLayout,
    wraps: Mutex<HashMap<(Option<Pixels>, Option<usize>), Arc<ArabicLayout>>>,
}
impl ArabicLayout {
    /// Whether the first visual line has right-to-left paragraph direction.
    pub fn is_rtl(&self) -> bool {
        self.layout
            .visual_lines
            .first()
            .is_some_and(|l| l.direction.is_rtl())
    }
    /// Resolve a UTF-8 caret and its bidi affinity to a visual rectangle.
    pub fn caret_bounds(&self, caret: CaretPosition, height: Pixels) -> Option<Bounds<Pixels>> {
        self.layout
            .platform_layout
            .caret_bounds(caret, cp(height))
            .map(bounds)
    }
    /// Hit-test a visual point, preserving the bidi caret affinity.
    pub fn caret_for_point(
        &self,
        p: Point<Pixels>,
        height: Pixels,
    ) -> Result<CaretPosition, CaretPosition> {
        self.layout
            .platform_layout
            .caret_from_pixel_point(ce::point(cp(p.x), cp(p.y)), cp(height))
    }
    /// Move one shaped cluster in the requested visual direction.
    pub fn adjacent_visual_caret(
        &self,
        caret: CaretPosition,
        direction: VisualDirection,
    ) -> Option<CaretPosition> {
        self.layout
            .platform_layout
            .adjacent_visual_caret(caret, direction)
    }
    /// Move to a visual word boundary.
    pub fn word_caret(&self, caret: CaretPosition, right: bool) -> CaretPosition {
        self.layout
            .platform_layout
            .caret_movement(
                caret,
                (if right {
                    TextDirection::Right
                } else {
                    TextDirection::Left
                })
                .with_boundary(TextBoundary::Word),
                None,
            )
            .result
    }
    /// Return the disjoint visual rectangles occupied by a source range.
    pub fn selection_bounds(
        &self,
        range: std::ops::Range<usize>,
        height: Pixels,
    ) -> Vec<Bounds<Pixels>> {
        self.layout
            .platform_layout
            .selection_bounds(range, cp(height))
            .into_iter()
            .map(bounds)
            .collect()
    }
    pub(crate) fn wrapped(
        self: &Arc<Self>,
        width: Option<Pixels>,
        max_lines: Option<usize>,
    ) -> Arc<Self> {
        // A fitting line already has the required shaping and interaction
        // geometry. Do not build a second Parley layout merely to measure it.
        if width.is_none_or(|width| width >= fp(self.layout.width))
            && max_lines.is_none_or(|limit| limit >= self.layout.visual_lines.len())
        {
            return self.clone();
        }
        let key = (width, max_lines);
        if let Some(cached) = self.wraps.lock().get(&key) {
            return cached.clone();
        }
        let options = ce::TextLayoutOptions {
            wrap_width: width.map(cp),
            line_clamp: max_lines,
            text_align: ce::TextAlign::Left,
            ..Default::default()
        };
        let layout = self.engine.layout_text(ce::TextLayoutRequest {
            text: &self.text,
            font_size: self.layout.font_size,
            runs: &self.runs,
            options,
        });
        let wrapped = Arc::new(Self {
            engine: self.engine.clone(),
            text: self.text.clone(),
            runs: self.runs.clone(),
            source_starts: self.source_starts.clone(),
            layout,
            wraps: Mutex::new(HashMap::new()),
        });
        let mut cache = self.wraps.lock();
        if cache.len() >= 4 {
            cache.clear();
        }
        cache.insert(key, wrapped.clone());
        wrapped
    }
}
struct CacheEntry {
    arabic: Weak<ArabicLayout>,
    owner: Option<Arc<ArabicLayout>>,
    fast: Option<Weak<LineLayout>>,
}
struct LayoutCache {
    entries: HashMap<u64, CacheEntry>,
    pending: VecDeque<Arc<ArabicLayout>>,
    sweep: VecDeque<u64>,
}
impl LayoutCache {
    fn collect_dead(&mut self) {
        // Bound reclamation work per insertion instead of scanning every live
        // layout in a periodic frame. Retained layouts are never evicted.
        for _ in 0..self.sweep.len().min(CLEANUP_BUDGET) {
            let key = self.sweep.pop_front().unwrap();
            let entry = self.entries.get_mut(&key).unwrap();
            if entry
                .fast
                .as_ref()
                .is_some_and(|weak| weak.strong_count() == 0)
            {
                entry.owner = None;
            }
            if entry.arabic.strong_count() == 0 {
                self.entries.remove(&key);
            } else {
                self.sweep.push_back(key);
            }
        }
    }
}
static LAYOUTS: LazyLock<Mutex<LayoutCache>> = LazyLock::new(|| {
    Mutex::new(LayoutCache {
        entries: HashMap::new(),
        pending: VecDeque::new(),
        sweep: VecDeque::new(),
    })
});
// The glyph buffer survives moving a LineLayout into Arc. Copies are registered
// explicitly, so caret lookup is constant-time and does not hash every glyph.
fn layout_key(layout: &LineLayout) -> u64 {
    layout
        .runs
        .iter()
        .find(|run| !run.glyphs.is_empty())
        .map_or(0, |run| run.glyphs.as_ptr() as usize as u64)
}
/// Retrieve the Arabic caret geometry for an existing Fast layout.
pub fn layout_for(layout: &LineLayout) -> Option<Arc<ArabicLayout>> {
    if !layout.runs.first().is_some_and(|r| tagged(r.font_id)) {
        return None;
    }
    LAYOUTS
        .lock()
        .entries
        .get(&layout_key(layout))
        .and_then(|entry| entry.arabic.upgrade())
}
fn remember(layout: &LineLayout, arabic: Arc<ArabicLayout>) {
    let key = layout_key(layout);
    let mut cache = LAYOUTS.lock();
    if !cache.entries.contains_key(&key) {
        cache.sweep.push_back(key);
    }
    cache.entries.insert(
        key,
        CacheEntry {
            arabic: Arc::downgrade(&arabic),
            owner: None,
            fast: None,
        },
    );
    // Bridge the short interval between platform shaping and the Fast cache
    // taking ownership. RecentShapes and InputLine retain their own geometry.
    cache.pending.push_back(arabic);
    while cache.pending.len() > PENDING_CAPACITY {
        cache.pending.pop_front();
    }
    cache.collect_dead();
}
pub(crate) fn copy_layout_metadata(source: &LineLayout, copy: &LineLayout) {
    if let Some(arabic) = layout_for(source) {
        remember(copy, arabic);
    }
}
pub(crate) fn track_layout(layout: &Arc<LineLayout>) {
    if !layout.runs.first().is_some_and(|r| tagged(r.font_id)) {
        return;
    }
    if let Some(entry) = LAYOUTS.lock().entries.get_mut(&layout_key(layout)) {
        entry.owner = entry.arabic.upgrade();
        entry.fast = Some(Arc::downgrade(layout));
    }
}

struct ResolvedFont {
    native: Font,
    arabic: OnceLock<ce::Font>,
}
impl ResolvedFont {
    fn arabic(&self) -> ce::Font {
        self.arabic.get_or_init(|| descriptor(&self.native)).clone()
    }
}

pub(crate) struct ArabicTextSystem {
    native: Arc<dyn PlatformTextSystem>,
    engine: OnceLock<Arc<arabic_parley::ParleyTextSystem>>,
    fonts: Mutex<HashMap<FontId, ResolvedFont>>,
    registered: Mutex<Vec<Cow<'static, [u8]>>>,
    rasters: Mutex<HashMap<RenderGlyphParams, ce::RasterizedGlyph>>,
}
impl ArabicTextSystem {
    pub(crate) fn new(native: Arc<dyn PlatformTextSystem>) -> Self {
        Self {
            native,
            engine: OnceLock::new(),
            fonts: Mutex::new(HashMap::new()),
            registered: Mutex::new(Vec::new()),
            rasters: Mutex::new(HashMap::new()),
        }
    }
    fn engine(&self) -> &Arc<arabic_parley::ParleyTextSystem> {
        if let Some(engine) = self.engine.get() {
            return engine;
        }
        let registered = self.registered.lock();
        self.engine.get_or_init(|| {
            let system = arabic_parley::ParleyTextSystem::new_with_system_font(
                arabic_parley::SystemFonts::Load,
                if cfg!(target_os = "macos") {
                    ".AppleSystemUIFont"
                } else {
                    "Noto Sans"
                },
            )
            .with_automatic_optical_sizing()
            .without_paragraph_result_cache()
            .with_fallback_families([
                "Noto Sans Arabic",
                "Arial",
                "Noto Sans",
                "DejaVu Sans",
            ]);
            system
                .add_fonts(registered.clone())
                .expect("register existing fonts with Arabic engine");
            Arc::new(system)
        })
    }
    fn raster(&self, params: &RenderGlyphParams) -> anyhow::Result<ce::RasterizedGlyph> {
        self.engine().rasterize_glyph(&ce::RenderGlyphParams {
            font_id: native_id(params.font_id),
            glyph_id: ce::GlyphId(params.glyph_id.0),
            font_size: cp(params.font_size),
            subpixel_variant: ce::point(params.subpixel_variant.x, params.subpixel_variant.y),
            scale_factor: params.scale_factor,
            raster_style: ce::PreparedRasterStyle::independent(if params.is_emoji {
                ce::GlyphRenderMode::Color
            } else {
                ce::GlyphRenderMode::Grayscale
            }),
        })
    }
}
impl PlatformTextSystem for ArabicTextSystem {
    fn add_fonts(&self, fonts: Vec<Cow<'static, [u8]>>) -> anyhow::Result<()> {
        self.native.add_fonts(fonts.clone())?;
        let mut registered = self.registered.lock();
        registered.extend(fonts.clone());
        if let Some(engine) = self.engine.get() {
            engine.add_fonts(fonts)?;
        }
        Ok(())
    }
    fn set_missing_glyph_sink(&self, sink: Option<Arc<dyn MissingGlyphSink>>) {
        self.native.set_missing_glyph_sink(sink)
    }
    fn all_font_names(&self) -> Vec<String> {
        self.native.all_font_names()
    }
    fn font_id(&self, font: &Font) -> anyhow::Result<FontId> {
        let id = self.native.font_id(font)?;
        self.fonts.lock().entry(id).or_insert_with(|| ResolvedFont {
            native: font.clone(),
            arabic: OnceLock::new(),
        });
        Ok(id)
    }
    fn prewarm_fonts(&self, ids: &[FontId]) {
        self.native.prewarm_fonts(ids)
    }
    fn font_metrics(&self, id: FontId) -> FontMetrics {
        if !tagged(id) {
            return self.native.font_metrics(id);
        }
        let m = self.engine().font_metrics(native_id(id));
        FontMetrics {
            units_per_em: m.units_per_em,
            ascent: m.ascent,
            descent: m.descent,
            line_gap: m.line_gap,
            underline_position: m.underline_position,
            underline_thickness: m.underline_thickness,
            cap_height: m.cap_height,
            x_height: m.x_height,
            bounding_box: Bounds::new(
                point(m.bounding_box.origin.x, m.bounding_box.origin.y),
                size(m.bounding_box.size.width, m.bounding_box.size.height),
            ),
        }
    }
    fn typographic_bounds(&self, id: FontId, glyph: GlyphId) -> anyhow::Result<Bounds<f32>> {
        if !tagged(id) {
            return self.native.typographic_bounds(id, glyph);
        }
        let b = self
            .engine()
            .typographic_bounds(native_id(id), ce::GlyphId(glyph.0))?;
        Ok(Bounds::new(
            point(b.origin.x, b.origin.y),
            size(b.size.width, b.size.height),
        ))
    }
    fn advance(&self, id: FontId, glyph: GlyphId) -> anyhow::Result<Size<f32>> {
        if !tagged(id) {
            return self.native.advance(id, glyph);
        }
        let s = self.engine().advance(native_id(id), ce::GlyphId(glyph.0))?;
        Ok(size(s.width, s.height))
    }
    fn glyph_for_char(&self, id: FontId, c: char) -> Option<GlyphId> {
        if !tagged(id) {
            return self.native.glyph_for_char(id, c);
        }
        self.engine()
            .glyph_for_char(native_id(id), c)
            .map(|g| GlyphId(g.0))
    }
    fn glyph_raster_bounds(
        &self,
        params: &RenderGlyphParams,
    ) -> anyhow::Result<Bounds<DevicePixels>> {
        if !tagged(params.font_id) {
            return self.native.glyph_raster_bounds(params);
        }
        let raster = self.raster(params)?;
        let b = raster.bounds;
        let mut rasters = self.rasters.lock();
        if rasters.len() >= 128 {
            rasters.clear();
        }
        rasters.insert(params.clone(), raster);
        Ok(Bounds::new(
            point(DevicePixels(b.origin.x.0), DevicePixels(b.origin.y.0)),
            size(DevicePixels(b.size.width.0), DevicePixels(b.size.height.0)),
        ))
    }
    fn rasterize_glyph(
        &self,
        params: &RenderGlyphParams,
        b: Bounds<DevicePixels>,
    ) -> anyhow::Result<(Size<DevicePixels>, Vec<u8>)> {
        if !tagged(params.font_id) {
            return self.native.rasterize_glyph(params, b);
        }
        let cached = self.rasters.lock().remove(params);
        let raster = match cached {
            Some(r) => r,
            None => self.raster(params)?,
        };
        Ok((
            size(
                DevicePixels(raster.size.width.0),
                DevicePixels(raster.size.height.0),
            ),
            raster.pixels,
        ))
    }
    fn layout_line(&self, text: &str, font_size: Pixels, runs: &[FontRun]) -> LineLayout {
        if !needs_bidi(text) {
            return self.native.layout_line(text, font_size, runs);
        }
        let engine = self.engine();
        let fonts = self.fonts.lock();
        let mut source_starts = Vec::new();
        let mut offset = 0;
        let ce_runs: Vec<_> = runs
            .iter()
            .map(|r| {
                source_starts.push(offset);
                offset += r.len;
                ce::TextRun {
                    len: r.len,
                    font: fonts.get(&r.font_id).expect("resolved Fast font").arabic(),
                    color: ce::black(),
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                    letter_spacing: None,
                }
            })
            .collect();
        drop(fonts);
        let native = engine.layout_text(ce::TextLayoutRequest {
            text,
            font_size: cp(font_size),
            runs: &ce_runs,
            options: ce::TextLayoutOptions {
                text_align: ce::TextAlign::Left,
                ..Default::default()
            },
        });
        let height = cp(font_size);
        let clusters = glyph_clusters(text, &native);
        let shaped_runs = native
            .paint_fragments
            .iter()
            .map(|fragment| ShapedRun {
                font_id: fast_id(fragment.font_id),
                glyphs: fragment
                    .glyphs
                    .iter()
                    .map(|glyph| ShapedGlyph {
                        id: GlyphId(glyph.id.0),
                        position: point(fp(glyph.position.x), fp(glyph.position.y)),
                        index: glyph_index(&native, clusters.as_deref(), glyph, height),
                        is_emoji: glyph.is_emoji,
                    })
                    .collect(),
            })
            .collect();
        let result = LineLayout {
            font_size,
            width: fp(native.width),
            ascent: fp(native.ascent),
            descent: fp(native.descent),
            runs: shaped_runs,
            len: text.len(),
        };
        remember(
            &result,
            Arc::new(ArabicLayout {
                engine: engine.clone(),
                text: Arc::from(text),
                runs: ce_runs,
                source_starts,
                layout: native,
                wraps: Mutex::new(HashMap::new()),
            }),
        );
        result
    }
    fn recommended_rendering_mode(&self, id: FontId, size: Pixels) -> TextRenderingMode {
        if !tagged(id) {
            return self.native.recommended_rendering_mode(id, size);
        }
        TextRenderingMode::Grayscale
    }
    fn glyph_dilation_for_color(&self, color: Hsla) -> u8 {
        self.native.glyph_dilation_for_color(color)
    }
}

pub(crate) fn wrap_boundaries(
    layout: &LineLayout,
    width: Pixels,
    max_lines: Option<usize>,
) -> Option<smallvec::SmallVec<[WrapBoundary; 1]>> {
    let arabic = layout_for(layout)?.wrapped(Some(width), max_lines);
    Some(
        arabic
            .layout
            .visual_lines
            .iter()
            .skip(1)
            .map(|row| {
                layout
                    .runs
                    .iter()
                    .enumerate()
                    .flat_map(|(ri, run)| {
                        run.glyphs
                            .iter()
                            .enumerate()
                            .map(move |(gi, g)| (ri, gi, g))
                    })
                    .min_by_key(|(_, _, glyph)| glyph.index.abs_diff(row.text_range.start))
                    .map(|(run_ix, glyph_ix, _)| WrapBoundary { run_ix, glyph_ix })
                    .unwrap_or(WrapBoundary {
                        run_ix: 0,
                        glyph_ix: 0,
                    })
            })
            .collect(),
    )
}
pub(crate) fn wrapped_layout(layout: &WrappedLineLayout) -> Option<Arc<ArabicLayout>> {
    Some(
        layout_for(&layout.unwrapped_layout)?
            .wrapped(layout.wrap_width, Some(layout.wrap_boundaries.len() + 1)),
    )
}
fn decoration_at(runs: &[DecorationRun], index: usize) -> Option<&DecorationRun> {
    let mut end = 0;
    for run in runs {
        end += run.len as usize;
        if index < end {
            return Some(run);
        }
    }
    runs.last()
}
fn align_offset(width: Pixels, available: Option<Pixels>, align: TextAlign) -> Pixels {
    let extra = available.unwrap_or(width) - width;
    match align {
        TextAlign::Left => px(0.),
        TextAlign::Center => extra / 2.,
        TextAlign::Right => extra,
    }
}
/// Paint shaped Arabic runs through Fast's existing batched glyph painter.
pub(crate) fn paint(
    arabic: &ArabicLayout,
    origin: Point<Pixels>,
    height: Pixels,
    align: TextAlign,
    width: Option<Pixels>,
    decorations: &[DecorationRun],
    background: bool,
    window: &mut Window,
    underline: &mut dyn FnMut(
        std::ops::Range<usize>,
        Point<Pixels>,
        Pixels,
        &UnderlineStyle,
        &mut Window,
    ),
) -> anyhow::Result<()> {
    let paint_bounds = Bounds::new(
        origin - point(fp(arabic.layout.font_size), px(0.)),
        size(
            width.unwrap_or(fp(arabic.layout.width)) + fp(arabic.layout.font_size) * 2.,
            height * arabic.layout.visual_lines.len().max(1),
        ),
    );
    window.paint_layer(paint_bounds, |window| {
        paint_in_layer(
            arabic,
            origin,
            height,
            align,
            width,
            decorations,
            background,
            window,
            underline,
        )
    })
}
#[allow(clippy::too_many_arguments)]
fn paint_in_layer(
    arabic: &ArabicLayout,
    origin: Point<Pixels>,
    height: Pixels,
    align: TextAlign,
    width: Option<Pixels>,
    decorations: &[DecorationRun],
    background: bool,
    window: &mut Window,
    underline: &mut dyn FnMut(
        std::ops::Range<usize>,
        Point<Pixels>,
        Pixels,
        &UnderlineStyle,
        &mut Window,
    ),
) -> anyhow::Result<()> {
    if background {
        let mut start = 0;
        for run in decorations {
            let end = start + run.len as usize;
            if let Some(color) = run.background_color {
                for mut b in arabic.selection_bounds(start..end, height) {
                    let row = (b.origin.y / height).floor() as usize;
                    let row_width = arabic
                        .layout
                        .visual_lines
                        .get(row)
                        .map_or(px(0.), |r| fp(r.advance_width));
                    b.origin += origin + point(align_offset(row_width, width, align), px(0.));
                    window.paint_quad(fill(b, color));
                }
            }
            start = end;
        }
        return Ok(());
    }
    let mut painter = crate::fast::glyphs::LineGlyphPainter::new(window);
    for (row_ix, row) in arabic.layout.visual_lines.iter().enumerate() {
        let row_origin = origin
            + point(
                align_offset(fp(row.advance_width), width, align),
                height * row_ix,
            );
        let baseline = (height - fp(arabic.layout.ascent) - fp(arabic.layout.descent)) / 2.
            + fp(arabic.layout.ascent);
        for fragment in &arabic.layout.paint_fragments[row.paint_fragment_range.clone()] {
            let source_start = arabic.source_starts[fragment.source_run];
            let decoration = decoration_at(decorations, source_start);
            let color = decoration.map_or_else(black, |d| d.color);
            for glyph in fragment.glyphs.iter() {
                let p = row_origin + point(fp(glyph.position.x), baseline + fp(glyph.position.y));
                if glyph.is_emoji {
                    window.paint_emoji(
                        p,
                        fast_id(fragment.font_id),
                        GlyphId(glyph.id.0),
                        fp(fragment.font_size),
                    )?;
                } else {
                    crate::fast::glyphs::LineGlyphPainter::paint_glyph(
                        &mut painter,
                        window,
                        p,
                        fast_id(fragment.font_id),
                        GlyphId(glyph.id.0),
                        fp(fragment.font_size),
                        color,
                    )?;
                }
            }
            if let Some(decoration) = decoration {
                let p = row_origin
                    + point(
                        fp(fragment.x_range.start),
                        baseline + fp(arabic.layout.descent) * 0.618,
                    );
                let span = fp(fragment.x_range.end - fragment.x_range.start);
                if let Some(style) = &decoration.underline {
                    underline(
                        source_start..source_start + arabic.runs[fragment.source_run].len,
                        p,
                        span,
                        style,
                        window,
                    );
                }
                if let Some(style) = &decoration.strikethrough {
                    window.paint_strikethrough(
                        row_origin
                            + point(
                                fp(fragment.x_range.start),
                                baseline - fp(arabic.layout.ascent) * 0.35,
                            ),
                        span,
                        style,
                    );
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{FONT_TAG, fast_id, native_id, needs_bidi, tagged};
    use crate::FontId;
    use arabic_core as ce;
    #[test]
    fn canonical_font_namespace_survives_the_renderer_boundary() {
        // Parley uses the high bit to reject non-canonical font IDs. Losing it
        // left all Arabic text invisible even though shaping/hit testing passed.
        for index in [0, 1, 37, 1024] {
            let canonical = ce::FontId(FONT_TAG | index);
            let renderer = fast_id(canonical);
            assert!(tagged(renderer));
            assert_eq!(native_id(renderer), canonical);
            assert!(!tagged(FontId(index)));
        }
    }
    #[test]
    fn ordinary_text_does_not_initialize_bidi_shaping() {
        for text in ["Order 123", "Café", "日本語", "🙂"] {
            assert!(!needs_bidi(text));
        }
        for text in ["سلام", "abc سلام 123", "מילים", "abc\u{2067}123\u{2069}"] {
            assert!(needs_bidi(text));
        }
    }
}
