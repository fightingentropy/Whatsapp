//! A hidden archive can only be revealed by a gesture that starts at the top.

const WHEEL_GESTURE_GAP: f64 = 0.20;
const MOMENTUM_START_GAP: f64 = 0.10;
const REVEAL_FRACTION: f32 = 0.55;

#[derive(Clone, Copy, PartialEq)]
enum PullSource {
    Wheel,
    Pointer,
}

#[derive(Clone, Copy)]
enum Motion {
    Pulling { source: PullSource, distance: f32 },
    Settling { target: f32, velocity: f32 },
}

#[derive(Clone, Default)]
pub(super) struct ArchivePull {
    pub(super) revealed: bool,
    extent: f32,
    motion: Option<Motion>,
    wheel_started_at_top: bool,
    last_wheel: Option<f64>,
    wheel_ended: bool,
    wheel_in_touch: bool,
    drag_started_at_top: bool,
    drag_position: Option<egui::Pos2>,
}

impl ArchivePull {
    pub(super) fn is_moving(&self) -> bool {
        self.motion.is_some()
    }

    /// While pulling or settling, owns the scroll offset so egui's smoothing
    /// and kinetic scrolling cannot apply the gesture a second time.
    pub(super) fn update(
        &mut self,
        ui: &egui::Ui,
        scroll_id: egui::Id,
        current: f32,
        height: f32,
    ) -> Option<f32> {
        let at_top = current <= height + 0.5;
        let over_list = ui.is_enabled()
            && ui.rect_contains_pointer(ui.available_rect_before_wrap())
            && !egui::Popup::is_any_open(ui.ctx());
        let drag_id = scroll_id.with("area");
        let dragging_list =
            ui.ctx().dragged_id() == Some(drag_id) || ui.ctx().drag_stopped_id() == Some(drag_id);
        let unclaimed_drag =
            ui.ctx().dragged_id().is_none() && ui.ctx().drag_stopped_id().is_none();
        let mut drag_rect = ui.available_rect_before_wrap();
        drag_rect.max.x -= ui.spacing().scroll.bar_width
            + ui.spacing().scroll.bar_inner_margin
            + ui.spacing().scroll.bar_outer_margin;
        let line_scroll_speed = ui
            .ctx()
            .options(|options| options.input_options.line_scroll_speed);
        let page_height = ui.available_height();
        let mut repaint_after = None;
        if !at_top {
            self.wheel_started_at_top = false;
            self.drag_started_at_top = false;
            self.drag_position = None;
        }
        let offset = ui.input(|input| {
            if self.pulling_from(PullSource::Wheel) && !self.wheel_in_touch {
                let remaining =
                    WHEEL_GESTURE_GAP - self.last_wheel.map_or(0.0, |last| input.time - last);
                if remaining <= 0.0 {
                    self.release(height, false);
                }
            }
            let pressed = input.pointer.primary_pressed();
            let released = input.pointer.primary_released();
            if pressed {
                self.drag_started_at_top = over_list && at_top && !self.revealed;
                self.drag_position = input.events.iter().find_map(|event| match event {
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        ..
                    } => Some(*pos),
                    _ => None,
                });
            }
            let position = if released {
                input.events.iter().find_map(|event| match event {
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed: false,
                        ..
                    } => Some(*pos),
                    _ => None,
                })
            } else {
                input.pointer.interact_pos()
            };
            // A whole drag can finish between frames before egui assigns a
            // dragged ID. It must still begin on the list, not its scrollbar.
            let completed_between_frames = pressed
                && released
                && unclaimed_drag
                && self
                    .drag_position
                    .is_some_and(|position| drag_rect.contains(position));
            if (dragging_list || completed_between_frames)
                && (input.pointer.primary_down() || released)
                && self.drag_started_at_top
                && let (Some(previous), Some(position)) = (self.drag_position, position)
            {
                // Use the press event's position, not the whole frame's delta:
                // moving to the press and dragging can arrive in one frame.
                self.pull_by(position.y - previous.y, height, PullSource::Pointer);
                self.drag_position = Some(position);
            }
            if !input.pointer.primary_down() {
                if self.pulling_from(PullSource::Pointer) {
                    self.release(height, false);
                }
                self.drag_started_at_top = false;
                self.drag_position = None;
            }
            for event in &input.events {
                let egui::Event::MouseWheel {
                    unit,
                    delta,
                    phase,
                    modifiers,
                } = event
                else {
                    continue;
                };
                let gap = self
                    .last_wheel
                    .map_or(f64::INFINITY, |last| input.time - last);
                // Winit reports macOS momentum as another Start immediately
                // after End. Keep that tail in the old gesture; it cannot
                // become a deliberate pull just because it reached the top.
                let momentum_start = self.wheel_ended && gap < MOMENTUM_START_GAP;
                let new_gesture = (!self.wheel_in_touch && gap > WHEEL_GESTURE_GAP)
                    || (*phase == egui::TouchPhase::Start && !momentum_start);
                if new_gesture {
                    self.wheel_started_at_top = over_list && at_top && !self.revealed;
                }
                if !over_list || modifiers.shift || delta.x.abs() > delta.y.abs() {
                    self.wheel_started_at_top = false;
                }
                if *phase == egui::TouchPhase::Move
                    && over_list
                    && delta.y > delta.x.abs()
                    && self.wheel_started_at_top
                {
                    let scale = match unit {
                        egui::MouseWheelUnit::Point => 1.0,
                        egui::MouseWheelUnit::Line => line_scroll_speed,
                        egui::MouseWheelUnit::Page => page_height,
                    };
                    self.pull_by(delta.y * scale, height, PullSource::Wheel);
                } else if over_list && delta.y < 0.0 {
                    // A reversed scroll should immediately return control to
                    // the list from its current position, without a jump.
                    self.motion = None;
                    self.revealed = current < height - 0.5;
                    self.wheel_started_at_top = false;
                }
                self.wheel_ended =
                    matches!(phase, egui::TouchPhase::End | egui::TouchPhase::Cancel);
                if *phase == egui::TouchPhase::Start {
                    self.wheel_in_touch = true;
                }
                if self.wheel_ended {
                    if self.pulling_from(PullSource::Wheel) {
                        self.release(height, *phase == egui::TouchPhase::Cancel);
                    }
                    self.wheel_in_touch = false;
                    self.wheel_started_at_top = false;
                }
                self.last_wheel = Some(input.time);
            }
            // macOS can deliver scrolling and dragging without keyboard
            // focus. Cancel only when focus is actually lost mid-gesture.
            if input.events.contains(&egui::Event::WindowFocused(false))
                && matches!(self.motion, Some(Motion::Pulling { .. }))
            {
                self.release(height, true);
                self.wheel_started_at_top = false;
                self.wheel_in_touch = false;
                self.drag_started_at_top = false;
                self.drag_position = None;
            }
            // Sense::drag takes ownership on press. Its frame delta may also
            // contain the move *to* the press position, which is not a pull.
            // Hold the offset for that frame, including native drag scrolling.
            let controlled = self.motion.is_some() || (pressed && self.drag_started_at_top);
            if let Some(Motion::Settling { target, velocity }) = self.motion {
                // Exact critically damped spring: stable at different refresh
                // rates, with no overshoot or permanent repaint loop.
                let dt = input.stable_dt.min(0.05);
                let spring = 26.0;
                let decay = (-spring * dt).exp();
                let displacement = self.extent - target;
                let step = (velocity + spring * displacement) * dt;
                self.extent = target + (displacement + step) * decay;
                let velocity = (velocity - spring * step) * decay;
                if (self.extent - target).abs() < 0.1 && velocity.abs() < 1.0 {
                    self.extent = target;
                    self.revealed = target > 0.0;
                    self.motion = None;
                } else {
                    self.motion = Some(Motion::Settling { target, velocity });
                    repaint_after = Some(0.0);
                }
            }
            // Plain wheels have no End event. Wake once at the end of the
            // burst; a held trackpad/pointer needs no idle frames.
            if self.pulling_from(PullSource::Wheel) && !self.wheel_in_touch {
                repaint_after = Some(
                    (WHEEL_GESTURE_GAP - self.last_wheel.map_or(0.0, |last| input.time - last))
                        as f32,
                );
            }
            controlled.then_some(height - self.extent)
        });
        if let Some(delay) = repaint_after {
            ui.ctx().request_repaint_after_secs(delay);
        }
        offset
    }

    fn pulling_from(&self, expected: PullSource) -> bool {
        matches!(self.motion, Some(Motion::Pulling { source, .. }) if source == expected)
    }

    fn pull_by(&mut self, delta: f32, height: f32, source: PullSource) {
        let distance = match self.motion {
            Some(Motion::Pulling { distance, .. }) => distance,
            // Re-grabbing a settling row starts at its visible position.
            _ if delta > 0.0 => -height * 1.4 * (1.0 - self.extent / height).max(0.001).ln(),
            _ => return,
        };
        let distance = (distance + delta).max(0.0);
        // Follow the fingers immediately, with increasing resistance as the
        // archive approaches its full height instead of hitting a hard stop.
        self.extent = height * (1.0 - (-distance / (height * 1.4)).exp());
        self.motion = Some(Motion::Pulling { source, distance });
    }

    fn release(&mut self, height: f32, cancelled: bool) {
        let target = if !cancelled && self.extent >= height * REVEAL_FRACTION {
            height
        } else {
            0.0
        };
        self.motion = Some(Motion::Settling {
            target,
            velocity: 0.0,
        });
    }

    pub(super) fn hide(&mut self) {
        self.revealed = false;
        self.extent = 0.0;
        self.motion = None;
        self.wheel_started_at_top = false;
        self.drag_started_at_top = false;
        self.drag_position = None;
    }
}
