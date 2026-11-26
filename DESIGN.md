# bside Design System — Obsidian Studio

## Philosophy
`bside` is a quiet, tactile desktop music studio & social listening companion (Linear × Apple Music macOS × Raycast). It is **not** a generic SaaS analytics dashboard.

### Non-Negotiable Anti-Slop Rules
- **NO pulsing / blinking status dots** (`animate-pulse`, `animate-ping`, `animate-live-dot`, blinking circles). Represent live playback through crisp typography, real-time progress bars, or a calm, subtle 3-bar audio level indicator.
- **NO hard neo-brutalist offset drop-shadows** or colored glow borders around cards.
- **NO neon Spotify-green outlines or green shadows** plastered over the UI. Reserve `#1DB954` strictly for the "Connect Spotify" OAuth action when unauthenticated.
- **NO badge/pill soup**: never stack 3+ pills/badges on a single card header. Metadata belongs in clean secondary text separated by `·`.
- **NO screaming ALL-CAPS monospace labels** on buttons, tabs, or section headings. Use clean Sentence Case in sans-serif; reserve monospace strictly for tabular numbers (`tabular-nums`), durations (`03:42`), timestamps, and CLI commands.
- **NO cards-inside-cards-inside-cards**: use hairline dividers (`1px solid var(--border-subtle)`), spacing rhythm, and surface contrast (`--bg-canvas` vs `--bg-surface`) instead of heavy boxes.

## Color Tokens (`web/src/index.css`)
- **Surfaces**:
  - `--bg-rail`: `#0A0A0C` (persistent left navigation rail)
  - `--bg-canvas`: `#0F0F12` (primary workspace)
  - `--bg-surface`: `#151519` (quiet card / panel surface)
  - `--bg-inset`: `#121216` (inputs, recessed bars, segmented wells)
  - `--bg-elevated`: `#1C1C22` (popovers, tooltips, dropdowns)
  - `--bg-hover`: `rgba(255, 255, 255, 0.045)`
  - `--bg-active`: `rgba(255, 255, 255, 0.085)`
- **Hairlines**:
  - `--border-subtle`: `rgba(255, 255, 255, 0.07)`
  - `--border-strong`: `rgba(255, 255, 255, 0.14)`
  - `--border-focus`: `rgba(255, 255, 255, 0.28)`
- **Ink**:
  - `--ink-primary`: `#F4F4F6`
  - `--ink-secondary`: `#A1A1AA`
  - `--ink-muted`: `#6E6E78`
  - `--ink-on-accent`: `#09090B`
- **Accents**:
  - `--accent-primary`: `#FAFAFA` (primary CTA & active control surface)
  - `--accent-ember`: `#E88B5A` (warm studio VU-meter accent for active audio progress, charts, highlights)
  - `--accent-ember-soft`: `rgba(232, 139, 90, 0.14)`
  - `--accent-blue`: `#7AA2F7` (secondary analytical series)
  - `--accent-sage`: `#86B89A` (tertiary series / completion)
  - `--danger`: `#E56B6F`
  - `--warning`: `#E5A952`
- **Ambient Color**: Album and playlist artwork provides color through subtle blurred ambient washes (`AmbientArtGlow`) behind hero headers and spotlight cards.

## Typography
- **UI & Headings**: `Inter`, `-apple-system`, `BlinkMacSystemFont`, sans-serif.
  - Page title: `22px–26px`, `font-semibold`, `tracking-[-0.025em]`
  - Section title: `15px`, `font-semibold`, `tracking-[-0.015em]`
  - Body / track title: `13px–14px`, `font-medium`
  - Metadata / artist: `12px`, `text-[var(--ink-secondary)]`
- **Numbers & Durations**: `tabular-nums` everywhere; `font-mono` only for durations (`03:42`), timestamps, and terminal commands.

## Motion & Tactile Feedback (`web/src/components/ui/motion.tsx`)
- Always wrap animations with `motion/react` (`MotionConfig reducedMotion="user"`).
- **Springs**:
  - `SPRING_SNAPPY` (`stiffness: 520, damping: 36, mass: 0.7`) — sliding active pills (`layoutId`), segmented controls, interactive toggles.
  - `SPRING_SMOOTH` (`stiffness: 320, damping: 30, mass: 0.85`) — view switches, drawer/panel reveals, detail panels.
- **Micro-interactions**:
  - Buttons & interactive rows have crisp hover surface transitions (`140ms`) and subtle tactile detent (`active:scale-[0.985]`).
  - Album artwork scales gently (`group-hover:scale-[1.03]`) with a 1px specular inner ring (`ring-1 ring-inset ring-white/10`).
  - Staggered reveals are fast (`25–35ms` per item, capped at `200ms` total) so switching views always feels instantaneous.

## Layout Architecture
- **Left Sidebar (`232px`)**: Brand header, primary navigation with `layoutId` sliding highlight, Active Circle (live friends preview — ready for Listen Along), and account footer.
- **Top Utility Bar (`52px`, sticky)**: Contextual page heading + active filter chips on the left; Friend filter, sliding timeframe control (`7d / 30d / 90d / All`), auto-refresh icon toggle, and refresh button on the right.
- **Main Scroll Canvas**: Generous breathing room (`max-w-[1200px]`, `px-6 py-6 sm:px-8`), leaving clean structural space at the bottom for the upcoming persistent Player + Listen Along dock.
