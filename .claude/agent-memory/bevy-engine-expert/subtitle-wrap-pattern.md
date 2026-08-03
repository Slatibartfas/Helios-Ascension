# Helios build-card subtitle pattern (Bevy 0.18)

`src/ui/construction.rs::spawn_card` renders the title + subtitle strip on every
mining/building card. The original card layout used `TextLayout::new_with_no_wrap()`
on the two marquee-track text copies and relied on a horizontal `UiTransform`
marquee to read truncated descriptions. This produced two visible bugs:

1. Long descriptions were **clipped at the right card edge** instead of
   wrapping — `Overflow::clip + no_wrap` is literally "horizontal cut-off".
2. The marquee never scrolled because its `text_width == container_width`
   invariant broke (the marquee track's intrinsic width pushed the clip
   wider than the title column; `tick_subtitle_marquee` reads
   `ComputedNode::size()` and saw no overflow → phase stayed at 0).

## Correct pattern (proven in v0.5.2 PR-B): wrap + marquee fallback

1. Use `TextLayout::new(Justify::Left, LineBreak::WordBoundary)` on both
   marquee-track text copies. Bevy soft-wraps at spaces inside the
   container's resolved width, so a 108-char description fills 2 lines
   inside the clip without horizontal cropping.

2. Pin the clip container with **explicit width AND height**:
   ```rust
   Node {
       width: Val::Percent(100.0),
       height: Val::Px(36.0),  // 2 × 12 px caption + line-height
       overflow: Overflow::clip(),
       ..default()
   }
   ```
   Without `width: 100%`, the marquee track's intrinsic width
   (sum of two text copies' content_size) inflates the clip past the
   title column and breaks the overflow detection invariant.

3. Keep the marquee machinery intact (`SubtitleMarquee`,
   `tick_subtitle_marquee`, two copies in a `flex_shrink: 0` track with
   a `UiTransform`). It is now a *fallback* for the pathological case
   where a single token (e.g. long chemical name) exceeds the column
   width. With wrapping enabled, `text_content_size().x` is bound by
   `container_width` so `overflows` stays false and the track sits
   dormant at translation `(0, 0)`.

4. Replace the mid-word 80-char clamp in `clamp_subtitle_two_lines`
   with a **word-boundary 140-char clamp**. The new helper backs up to
   the previous whitespace before appending `…` so `chalcopyrite`
   never becomes `chalcopy…`.

## Why 36 px, not 28 px
The old 28 px budget was tight for 12-px caption + default line-height
(1.0–1.1) but marginal for the 1.5 line-height Bevy 0.18's parley stack
actually computes. 36 px gives two full lines of breathing room and
still leaves the ETA / Queue CTA reservation beneath intact (parent card
heights unchanged).

## Companion rule
The row containing the clip (`card_title_col` → flex column child)
provides the **cross-axis stretch** for the clip. Stretch on the flex
column's cross axis means "take the column's cross-size". That is the
underlying reason explicit `width: 100%` is needed on the clip itself:
without it, the clip's own intrinsic width (driven by its marquee
track child) competes with the column's stretch and the behaviour
depends on `align-items` of the parent — fragile and component-version
sensitive.
