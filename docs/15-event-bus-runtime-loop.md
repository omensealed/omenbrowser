# 15 — Event Bus and Runtime Loop

OMENbrowser_rs must remain responsive while doing network work. This document defines the event architecture required to finish the port.

## Problem

By Phase 14, browser loads, message sends, and message syncs are non-blocking tasks, but the app still needs a unified path for:

- crossterm keyboard/mouse input;
- browser task results;
- message task results;
- Reticulum announces;
- LXMF inbound messages;
- delivery status updates;
- partial refresh timers;
- diagnostics updates;
- plugin events;
- logs/toasts/errors.

Do not add one-off channels for every new feature forever. Create one app-level event bus.

## Core event types

Create or consolidate:

```rust
pub enum AppEvent {
    Input(InputEvent),
    Tick(TickKind),
    Browser(BrowserEvent),
    Message(MessageEvent),
    Runtime(RuntimeEvent),
    Directory(DirectoryEvent),
    Plugin(PluginEvent),
    Diagnostics(DiagnosticsEvent),
    Log(LogEvent),
    Shutdown,
}
```

Keep detailed sub-events in subsystem modules.

## RuntimeEvent examples

```rust
pub enum RuntimeEvent {
    StatusChanged(NetworkStatus),
    Announce(AnnounceEvent),
    PathUpdated(PathEvent),
    MessageReceived(MessageEnvelope),
    MessageDeliveryUpdated(DeliveryUpdate),
    PropagationStatus(PropagationStatus),
    InterfaceStats(Vec<InterfaceStats>),
    Debug(RuntimeDebugEvent),
    Error(RuntimeErrorEvent),
}
```

## Channel design

Recommended:

- `tokio::sync::mpsc` for app event queue.
- `tokio::sync::broadcast` only for internal runtime fanout if needed.
- `watch` channels for latest status snapshots if useful.

The TUI loop should own the receiver and drain events in batches before each render.

## Main loop shape

Conceptual shape:

```rust
loop {
    tokio::select! {
        Some(event) = app_event_rx.recv() => {
            app.handle_event(event).await?;
        }
        _ = render_interval.tick() => {
            app.handle_event(AppEvent::Tick(TickKind::Render)).await?;
            terminal.draw(|frame| ui::draw(frame, &app))?;
        }
    }

    while let Ok(event) = app_event_rx.try_recv() {
        app.handle_event(event).await?;
    }
}
```

Do not do long network work inside `handle_event`. Schedule tasks and return.

## Crossterm input bridge

Input reading can stay in a blocking helper thread if necessary, but it should send `AppEvent::Input` into the async queue.

Requirements:

- terminal teardown must always run;
- panic path should restore terminal mode;
- input task should stop on shutdown;
- mouse capture remains paired with terminal guard.

## Runtime bridge

Native runtime should not mutate app state directly.

Correct path:

```text
native runtime event -> runtime adapter -> RuntimeEvent -> AppEvent::Runtime -> app.handle_runtime_event -> service/store/UI state
```

## Event handling ownership

| Event | Handler |
|---|---|
| keyboard/mouse | `App::handle_input_event` |
| browser result | `App::handle_browser_event` |
| message result | `App::handle_message_event` |
| announce | `DirectoryService`, then UI refresh |
| inbound message | `MessagingService`, then tab update |
| delivery update | `MessagingService`, then tab update |
| interface stats | runtime status + diagnostics refresh |
| plugin hook result | plugin service then target subsystem |
| log/error | log buffer + toast/status |

## Backpressure

Network stacks can produce bursts of announces/logs.

Rules:

- Event queue must have a bounded capacity.
- Droppable debug/log events may be dropped when full.
- Message received and delivery events must not be silently dropped.
- Announce bursts may be coalesced by destination hash.
- Status updates may be coalesced by taking latest.

## Timers

Use named timers rather than scattered sleeps.

Suggested timers:

- render tick: 30–60 FPS max, lower if idle;
- status tick: 1–2 seconds;
- partial refresh tick: based on per-partial schedule;
- message sync tick: configurable/off by default until live runtime stable;
- diagnostics tick: 5–10 seconds or on demand;
- autosave/debounce tick: for directory/settings if needed.

## Cancellation

Every scheduled network operation must have:

- operation id;
- tab/conversation id where applicable;
- generation number;
- cancellation token;
- timeout.

Late results must be ignored if generation does not match.

## Tests

Add tests for:

- event ordering does not corrupt active tab;
- stale browser event ignored;
- stale message event ignored;
- runtime announce updates directory only once;
- inbound duplicate message does not duplicate visible thread;
- cancelled task result ignored;
- shutdown sends stop signal and exits loop cleanly.

## Done when

- New live runtime events do not require UI-specific channels.
- Manual refresh shortcuts still work.
- Live announces/messages can enter the app without freezing.
- All existing non-blocking behavior still passes tests.
