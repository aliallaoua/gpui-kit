# Patched GPUI Fast runtime

Source: the official crates.io `gpui-fast` 0.1.6 archive from Longbridge.
Archive SHA-256: `41e1053e2088b5c249aa78b50642e61323ed80afebbec1e5b317871eec340c7f`.
Upstream: https://github.com/longbridge/gpui-fast.

The upstream Apache-2.0 LICENSE and copyright notices are retained. The
published archive contains no NOTICE file. This copy is not published to
crates.io. Platform crates remain the official matching 0.1.6 releases.

Local modifications add the optional `arabic` feature and `src/arabic.rs`,
connect it to the text system, caret queries, wrapping and painting, and keep
Arabic geometry alive with Fast's recent-shape cache. Parley's existing
canonical font IDs are preserved across the renderer boundary. No Arabic
engine source is copied: the feature uses the existing pinned private
`holool/gpui-arabic` dependency. Its repository access is required to build.

Retained views, layout retention, scene composition, number shaping and glyph
atlas implementation remain upstream. Without the `arabic` feature, the
new runtime hooks compile out. Ordinary text with the feature enabled delegates
to Fast's original platform shaper; the Parley engine initializes lazily.
