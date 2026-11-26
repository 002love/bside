# bside Design System — True Dark Full-Bleed Studio

## Philosophy
`bside` is a **true-dark, full-bleed desktop music studio** (Spotify Desktop × Apple Music macOS × Linear). Content paints directly across the full width of the canvas with **vivid, unmistakable dynamic accent colors extracted from real track/playlist/profile artwork**.

### Hard Rules (Anti-Slop & Anti-Washed-Out)
1. **True Deep Dark, NEVER muddy gray**:
   - Left rail: `#000000` (`--bg-rail`)
   - Main canvas: `#09090b` (`--bg-canvas`)
   - Primary ink: `#ffffff` (`--ink-primary`) — crisp contrast, never dusty gray
   - Secondary ink: `#b3b3bd` (`--ink-secondary`)
2. **NO Duplicate Headers**:
   - `App.tsx` owns ONLY a slim translucent floating utility bar (`FriendMultiSelect`, `7d/30d/90d/All`, Refresh) with **NO page title**.
   - Each view owns its own single, confident edge-to-edge hero or header directly at the top of the canvas.
3. **Full-Bleed Edge-to-Edge Canvas**:
   - NEVER center the app in a narrow webpage column (`max-w-[1220px] mx-auto` is banned!).
   - Views span `w-full` edge-to-edge across the right pane with consistent `px-6 lg:px-8` inner gutters and full-bleed color-washed hero bands (`-mx-*` or full-width hero sections that touch both edges and top!).
4. **Cardless by Default**:
   - Do NOT wrap lists, tables, charts, or directories in `.bside-card` boxes!
   - Paint lists, tracklists, history streams, and charts directly on the deep dark `#09090b` canvas using clean spacing, subtle `1px solid rgba(255,255,255,0.075)` hairlines, and crisp `hover:bg-white/[0.055]` row feedback.
   - Reserve enclosed surfaces strictly for floating popovers, inputs, or compact interactive spotlights where separation truly helps.
5. **Real Dynamic Accent Colors Everywhere (`web/src/lib/extractColor.ts`)**:
   - Use `useCoverAccent(imageUrl, seed, spotifyColorInt)` on tracks, playlists, and friend profiles!
   - Spread the extracted `palette.accent` color directly to:
     - Full-bleed hero gradient washes (`<AmbientArtGlow palette={palette} intensity="vivid" />`)
     - Playback progress bars (`backgroundColor: palette.accent` + subtle glow)
     - Dancing audio bars (`<PixelEq active color={palette.accent} />`)
     - Live avatar rings (`<UserAvatar isLive ringColor={palette.accent} />`)
     - Play-count / match / rank bars and active row accents
6. **Zero AI-Slop Tropes**:
   - NO pulsing/blinking dots (`animate-pulse`, `animate-ping`).
   - NO ALL-CAPS monospace UI labels/buttons. Use Sentence case sans-serif; reserve `font-mono tabular-nums` strictly for durations (`03:42`), timestamps, and CLI commands.
   - NO badge/pill soup.

## Motion (`web/src/components/ui/motion.tsx`)
- Keep crisp spring animations (`SPRING_SNAPPY` for `layoutId` pills, `SPRING_SMOOTH` for layout/view transitions, `AnimatePresence mode="wait"` for stage switches, `tactile-press` on interactive buttons/rows, `hoverZoom` on cover art).
