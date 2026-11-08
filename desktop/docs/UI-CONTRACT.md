# Native UI behavior and review

The desktop window uses eframe/egui 0.33.3 with the native Glow renderer. All labels and operational controls are German. Charcoal surfaces, cyan readings, and amber unavailable/error states distinguish measurements from drafts.

## Deliberate device operations

- The worker discovers devices at launch. Exactly one discovered controller may be connected automatically; this performs status queries only.
- Selecting a tab, moving a slider, editing a curve, changing RGB settings, loading a profile, or saving a profile sends no device-setting command.
- `Lüfter`: pick F1–F6, edit a manual PWM draft, then use that channel's **Übernehmen** button. Six channels are shown; F6 is identified as the pump-compatible mapping, not as proof that a physical pump is attached.
- Each channel can instead use a T1–T3 curve. Editing permits 2–16 points, with invalid order rejected by profile validation. **Kurven starten** is explicit, enabled only when connected and at least one curve channel is selected. Defaults and loaded profiles never start curves.
- During active curves, manual apply is disabled. Draft edits do not modify the running worker profile. **Kurven stoppen → 100 %** asks the worker to restore only active curve channels to 100%; communication failure can prevent that fallback.
- `Beleuchtung` provides the nine documented global effects and 0–100 speed/brightness. **Beleuchtung übernehmen** sends settings. There are no invented per-port or internal-LED selectors.
- `Profile` displays the native per-user path. Save/load are explicit; malformed files show an error and remain intact. Load changes the draft, not current hardware or active curves.
- Disconnect and window shutdown use the service's bounded cleanup behavior. No autostart, system service or silent background control is created.

## Real data and availability

Real mode begins with no fabricated values. T1–T3 show `— °C` until available; disconnected probes and absent snapshots are distinguished. RPM and PWM targets reflect the last successful snapshot, with its age displayed. Readings older than six seconds are marked stale. FLO/LVL show raw values, with calibration/presence explicitly unconfirmed. A fatal worker disconnect clears the displayed snapshot. Errors and operation results remain visible in the bottom status area.

Demo mode is selected explicitly by `--demo`, prominently marked **DEMO · simulierte Daten**, and uses the worker's isolated in-memory controller. It is never an automatic fallback for missing hardware.

## Screenshots and verification

The main CLI exposes `--demo --capture <PNG> --width <W> --height <H>` (see `--help` for final CLI names). `GuiOptions` accepts the demo flag, optional capture path and initial logical window size. Capture without demo is rejected. After stable demo frames, egui's actual renderer screenshot event is saved as RGBA PNG; success then closes the window. Save errors, a missing renderer response, or failure to obtain stable demo measurements produce diagnostics and a non-success result. Closing before capture completes is also an error.

Required native screenshot sizes are 960×720 and 1280×900. The application has a 760×560 minimum size and vertically scrollable page content. Native captures are separate from the headless tests and do not establish physical USB operation.

Headless tests render all four tabs at both requested sizes without a device snapshot, verify keyboard Tab reaches a control, and verify unavailable/NaN temperatures never become numeric measurements. Main integration additionally runs protocol, worker, profile and curve tests. Device writes are not needed for UI verification.

## Contract review notes

No blocking conflict was found. The worker must emit `Disconnected` after a poisoned transport because `Error` alone does not distinguish fatal device errors from local validation errors; the agreed service contract implements this. A missing sensor's 100% curve fallback is worker behavior, not a GUI-only promise. Windows hardware testing and physical fan/probe calibration remain distinct acceptance gates.
