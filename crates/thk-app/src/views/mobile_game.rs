use crate::fx3d::{paint_icon_3d, Icon3dKind};
use crate::state::{LauncherAppState, ToastKind};
use crate::theme::{
    accent_palette, readable_accent_ink, ACCENT_AMBER, ACCENT_BLUE, ACCENT_CYAN, ACCENT_EMERALD,
    ACCENT_RED, BG_CARD_LIGHT, BG_SKY_LIGHT, BORDER_DARK_CONTOUR, TEXT_INK_LIGHT, TEXT_MUTED_LIGHT,
};
use egui::{
    pos2, vec2, Align2, Color32, CornerRadius, FontId, Frame, Id, Margin, Order, Pos2, Rect,
    RichText, ScrollArea, Sense, Stroke, StrokeKind, TouchPhase,
};
use std::collections::{HashMap, HashSet};
use std::time::Instant;
use thk_core::mobile_controls::{
    bindable_catalog, ControlBinding, ControlShape, MobileControlButton, MobileControlLayout,
    GLFW_KEY_A, GLFW_KEY_D, GLFW_KEY_ENTER, GLFW_KEY_LEFT_SHIFT, GLFW_KEY_S, GLFW_KEY_T,
    GLFW_KEY_W, GLFW_MOUSE_BUTTON_LEFT, GLFW_MOUSE_BUTTON_RIGHT,
};
use thk_launcher::{MobileInputEvent, MobileSessionHandle};
use uuid::Uuid;

#[derive(Debug, Clone)]
enum TouchRole {
    Control {
        button_idx: usize,
    },
    Trackpad {
        start_pos: Pos2,
        last_pos: Pos2,
        started_at: Instant,
        hold_triggered: bool,
        holding_left_break: bool,
    },
}

pub struct MobileGameUiState {
    pub screen_open: bool,
    pub hub_visible_over_game: bool,
    pub controls_hidden: bool,
    pub edit_mode: bool,
    pub selected_btn_idx: Option<usize>,
    pub virtual_mouse_forced: bool,
    pub cursor_norm: (f32, f32),
    pub camera_accum_px: (f32, f32),
    pub mouse_drag_lock: bool,
    pub chat_bar_open: bool,
    pub chat_draft: String,
    pub inline_log_open: bool,
    pub floating_menu_pos: Option<Pos2>,
    pub floating_menu_expanded: bool,
    active_touches: HashMap<u64, TouchRole>,
    pointer_fallback_active: bool,
    prev_held_keys: HashSet<i32>,
    prev_held_mouse: HashSet<i32>,
    toggled_button_ids: HashSet<String>,
    crouch_toggled: bool,
    crouch_touch_latch: bool,
    toggle_press_latches: HashSet<String>,
    pending_release_keys: Vec<i32>,
    pending_release_mouse: Vec<i32>,
    last_sent_window_size: (u32, u32),
}

impl Default for MobileGameUiState {
    fn default() -> Self {
        Self {
            screen_open: false,
            hub_visible_over_game: false,
            controls_hidden: false,
            edit_mode: false,
            selected_btn_idx: None,
            virtual_mouse_forced: false,
            cursor_norm: (0.5, 0.5),
            camera_accum_px: (640.0, 360.0),
            mouse_drag_lock: false,
            chat_bar_open: false,
            chat_draft: String::new(),
            inline_log_open: false,
            floating_menu_pos: None,
            floating_menu_expanded: false,
            active_touches: HashMap::new(),
            pointer_fallback_active: false,
            prev_held_keys: HashSet::new(),
            prev_held_mouse: HashSet::new(),
            toggled_button_ids: HashSet::new(),
            crouch_toggled: false,
            crouch_touch_latch: false,
            toggle_press_latches: HashSet::new(),
            pending_release_keys: Vec::new(),
            pending_release_mouse: Vec::new(),
            last_sent_window_size: (0, 0),
        }
    }
}

impl MobileGameUiState {
    pub fn reset_transient_inputs(&mut self, session: Option<&MobileSessionHandle>) {
        if let Some(sess) = session {
            for &k in &self.prev_held_keys {
                sess.send(MobileInputEvent::Key {
                    keycode: k,
                    down: false,
                });
            }
            for &m in &self.prev_held_mouse {
                sess.send(MobileInputEvent::MouseButton {
                    button: m,
                    down: false,
                });
            }
        }
        self.active_touches.clear();
        self.pointer_fallback_active = false;
        self.prev_held_keys.clear();
        self.prev_held_mouse.clear();
        self.toggled_button_ids.clear();
        self.crouch_toggled = false;
        self.crouch_touch_latch = false;
        self.toggle_press_latches.clear();
        self.mouse_drag_lock = false;
    }
}

pub(crate) fn button_rect_on_canvas(btn: &MobileControlButton, canvas: Rect) -> Rect {
    let w = (btn.width_dp * btn.scale).clamp(22.0, (canvas.width() * 0.62).max(24.0));
    let h = (btn.height_dp * btn.scale).clamp(22.0, (canvas.height() * 0.62).max(24.0));
    let raw_cx = canvas.left() + btn.x_norm.clamp(0.0, 1.0) * canvas.width();
    let raw_cy = canvas.top() + btn.y_norm.clamp(0.0, 1.0) * canvas.height();
    let edge_pad = 4.0;
    let min_cx = canvas.left() + w * 0.5 + edge_pad;
    let max_cx = (canvas.right() - w * 0.5 - edge_pad).max(min_cx);
    let min_cy = canvas.top() + h * 0.5 + edge_pad;
    let max_cy = (canvas.bottom() - h * 0.5 - edge_pad).max(min_cy);
    Rect::from_center_size(
        pos2(raw_cx.clamp(min_cx, max_cx), raw_cy.clamp(min_cy, max_cy)),
        vec2(w, h),
    )
}

fn hit_test_control(pos: Pos2, buttons: &[MobileControlButton], canvas: Rect) -> Option<usize> {
    for (idx, btn) in buttons.iter().enumerate().rev() {
        let rect = button_rect_on_canvas(btn, canvas);
        match btn.shape {
            ControlShape::Round => {
                let radius = rect.width().min(rect.height()) * 0.54;
                if rect.center().distance(pos) <= radius {
                    return Some(idx);
                }
            }
            ControlShape::Rect | ControlShape::Dpad => {
                if rect.expand(3.0).contains(pos) {
                    return Some(idx);
                }
            }
        }
    }
    None
}

fn dpad_cell_at(pos: Pos2, dpad_rect: Rect) -> Option<(usize, usize)> {
    let expanded = dpad_rect.expand(12.0);
    if !expanded.contains(pos) {
        return None;
    }
    let rel_x = ((pos.x - dpad_rect.left()) / dpad_rect.width().max(1.0)).clamp(0.0, 0.999);
    let rel_y = ((pos.y - dpad_rect.top()) / dpad_rect.height().max(1.0)).clamp(0.0, 0.999);
    let col = (rel_x * 3.0) as usize;
    let row = (rel_y * 3.0) as usize;
    Some((row.min(2), col.min(2)))
}

pub fn render_mobile_game_screen(app: &mut LauncherAppState, ui: &mut egui::Ui) {
    let ctx = ui.ctx().clone();
    let active_session = app.instance_supervisor.active_mobile_session();
    let session_handle = active_session.as_ref().map(|(_, s)| s.clone());
    let game_running = !app.instance_supervisor.running_keys().is_empty();

    let central_frame = if game_running {
        Frame::NONE
    } else if ui.visuals().dark_mode {
        Frame::NONE.fill(Color32::from_rgb(11, 15, 24))
    } else {
        Frame::NONE.fill(BG_SKY_LIGHT)
    };

    egui::CentralPanel::default()
        .frame(central_frame)
        .show(ui, |ui| {
            let full_canvas = ui.max_rect();
            let canvas = crate::views::apply_safe_area_to_rect(
                full_canvas,
                crate::views::current_safe_area_insets(&ctx),
            );
            let ppp = ctx.pixels_per_point().max(1.0);
            let scale_pct = thk_core::models::sanitize_mobile_render_scale_pct(
                app.settings.mobile_render_scale_pct,
            );
            let target_fb_w =
                (((full_canvas.width() * ppp) as u32 * scale_pct / 100).max(320)) & !1;
            let target_fb_h =
                (((full_canvas.height() * ppp) as u32 * scale_pct / 100).max(240)) & !1;

            if let Some(ref sess) = session_handle {
                if app.mobile_game.last_sent_window_size != (target_fb_w, target_fb_h) {
                    app.mobile_game.last_sent_window_size = (target_fb_w, target_fb_h);
                    sess.send(MobileInputEvent::WindowSize {
                        width: target_fb_w,
                        height: target_fb_h,
                    });
                }
            }

            flush_pending_pulse_releases(&mut app.mobile_game, session_handle.as_ref());

            if !game_running {
                paint_preview_backdrop(app, ui, full_canvas, canvas);
            }

            let is_grabbing = session_handle
                .as_ref()
                .is_some_and(MobileSessionHandle::is_grabbing);
            let virtual_mouse_active = !app.mobile_game.edit_mode
                && (!is_grabbing || app.mobile_game.virtual_mouse_forced);

            let mut reserved_rects = Vec::with_capacity(4);
            if app.mobile_game.edit_mode {
                let bar_h = 84.0;
                reserved_rects.push(Rect::from_min_size(canvas.min, vec2(canvas.width(), bar_h)));
            }
            if app.mobile_game.chat_bar_open {
                let chat_x = if canvas.width() < 520.0 {
                    canvas.left() + 8.0
                } else {
                    canvas.left() + canvas.width() * 0.16
                };
                let chat_w = if canvas.width() < 520.0 {
                    (canvas.width() - 16.0).max(120.0)
                } else {
                    canvas.width() * 0.68
                };
                let chat_rect =
                    Rect::from_min_size(pos2(chat_x, canvas.top() + 44.0), vec2(chat_w, 46.0));
                reserved_rects.push(chat_rect);
            }
            if virtual_mouse_active && !app.mobile_game.controls_hidden {
                let dock_rect = virtual_mouse_dock_rect(canvas);
                reserved_rects.push(dock_rect);
            }

            if app.mobile_game.edit_mode {
                render_edit_mode_canvas(app, ui, canvas);
            } else {
                process_live_touch_and_controls(
                    app,
                    ui,
                    canvas,
                    &reserved_rects,
                    session_handle.as_ref(),
                    virtual_mouse_active,
                    (target_fb_w.max(1) as f32, target_fb_h.max(1) as f32),
                );
            }

            if virtual_mouse_active {
                paint_virtual_mouse_cursor(ui, full_canvas, app.mobile_game.cursor_norm);
                if !app.mobile_game.controls_hidden {
                    render_virtual_mouse_dock(
                        app,
                        &ctx,
                        canvas,
                        session_handle.as_ref(),
                        (target_fb_w.max(1) as f32, target_fb_h.max(1) as f32),
                    );
                }
            }

            if app.mobile_game.chat_bar_open {
                render_inline_chat_bar(app, &ctx, canvas, session_handle.as_ref());
            }

            if app.mobile_game.inline_log_open {
                render_inline_log_overlay(app, &ctx, canvas);
            }
        });

    render_mobile_floating_menu(app, &ctx);

    if game_running {
        ctx.request_repaint();
    }
}

fn flush_pending_pulse_releases(
    state: &mut MobileGameUiState,
    session: Option<&MobileSessionHandle>,
) {
    if let Some(sess) = session {
        for keycode in state.pending_release_keys.drain(..) {
            sess.send(MobileInputEvent::Key {
                keycode,
                down: false,
            });
        }
        for button in state.pending_release_mouse.drain(..) {
            sess.send(MobileInputEvent::MouseButton {
                button,
                down: false,
            });
        }
    } else {
        state.pending_release_keys.clear();
        state.pending_release_mouse.clear();
    }
}

fn paint_preview_backdrop(
    app: &mut LauncherAppState,
    ui: &mut egui::Ui,
    full_canvas: Rect,
    safe_canvas: Rect,
) {
    let dark = ui.visuals().dark_mode;
    let painter = ui.painter();
    let grid_stroke = if dark {
        Stroke::new(1.0, Color32::from_rgba_unmultiplied(45, 212, 191, 22))
    } else {
        Stroke::new(1.0, Color32::from_rgba_unmultiplied(59, 130, 246, 36))
    };
    let step = 48.0;
    let mut x = full_canvas.left();
    while x <= full_canvas.right() {
        painter.line_segment(
            [pos2(x, full_canvas.top()), pos2(x, full_canvas.bottom())],
            grid_stroke,
        );
        x += step;
    }
    let mut y = full_canvas.top();
    while y <= full_canvas.bottom() {
        painter.line_segment(
            [pos2(full_canvas.left(), y), pos2(full_canvas.right(), y)],
            grid_stroke,
        );
        y += step;
    }

    let center = safe_canvas.center();
    let cross_col = if dark {
        Color32::from_rgba_unmultiplied(255, 255, 255, 110)
    } else {
        Color32::from_rgba_unmultiplied(27, 40, 56, 135)
    };
    painter.line_segment(
        [
            pos2(center.x - 9.0, center.y),
            pos2(center.x + 9.0, center.y),
        ],
        Stroke::new(1.6, cross_col),
    );
    painter.line_segment(
        [
            pos2(center.x, center.y - 9.0),
            pos2(center.x, center.y + 9.0),
        ],
        Stroke::new(1.6, cross_col),
    );

    if !app.mobile_game.edit_mode {
        let has_progress = app.active_progress.is_some();
        let max_bw = (safe_canvas.width() - 16.0).max(116.0);
        let banner_w = (safe_canvas.width() * 0.88)
            .min(max_bw)
            .clamp(250.0_f32.min(max_bw), 520.0_f32.min(max_bw));
        let target_bh = if has_progress { 88.0_f32 } else { 68.0_f32 };
        let banner_h = target_bh.min((safe_canvas.height() - 48.0).max(58.0));
        let min_cy = safe_canvas.top() + banner_h * 0.5 + 40.0;
        let max_cy = (safe_canvas.bottom() - banner_h * 0.5 - 8.0).max(min_cy);
        let banner_cy = (safe_canvas.top() + 104.0).clamp(min_cy, max_cy);
        let banner_rect = Rect::from_center_size(
            pos2(safe_canvas.center().x, banner_cy),
            vec2(banner_w, banner_h),
        );
        let (banner_fill, banner_stroke, title_col, sub_col) = if dark {
            (
                Color32::from_rgba_unmultiplied(15, 23, 38, 222),
                Stroke::new(1.0, Color32::from_rgba_unmultiplied(16, 185, 129, 130)),
                Color32::WHITE,
                Color32::from_rgb(176, 192, 214),
            )
        } else {
            (
                BG_CARD_LIGHT,
                Stroke::new(1.6, BORDER_DARK_CONTOUR),
                TEXT_INK_LIGHT,
                TEXT_MUTED_LIGHT,
            )
        };
        painter.rect(
            banner_rect,
            CornerRadius::same(12),
            banner_fill,
            banner_stroke,
            StrokeKind::Inside,
        );

        let banner_painter = painter.with_clip_rect(
            ui.clip_rect()
                .intersect(banner_rect.shrink2(vec2(6.0, 2.0))),
        );
        let prof_name = app
            .selected_profile()
            .map(|p| format!("{} ({})", p.name, p.game_version))
            .unwrap_or_else(|| {
                app.i18n
                    .tr("Chưa chọn Profile", "No Profile Selected")
                    .to_string()
            });
        banner_painter.text(
            pos2(banner_rect.center().x, banner_rect.top() + 18.0),
            Align2::CENTER_CENTER,
            format!(
                "{}: {prof_name}",
                app.i18n.tr("Màn hình Run Game", "Mobile Game Screen")
            ),
            FontId::proportional(14.0),
            title_col,
        );

        if let Some(ref p) = app.active_progress {
            banner_painter.text(
                pos2(banner_rect.center().x, banner_rect.top() + 39.0),
                Align2::CENTER_CENTER,
                format!("{} ({:.0}%)", p.message, p.percent),
                FontId::proportional(11.5),
                sub_col,
            );
        }

        let btn_w = 168.0_f32.min((banner_w - 16.0).max(96.0));
        let btn_rect = Rect::from_center_size(
            pos2(banner_rect.center().x, banner_rect.bottom() - 16.0),
            vec2(btn_w, 26.0),
        );
        let btn_resp = ui.interact(
            btn_rect,
            ui.make_persistent_id("mobile_preview_launch_btn"),
            Sense::click(),
        );
        let btn_fill = if btn_resp.hovered() {
            Color32::from_rgb(34, 211, 102)
        } else {
            ACCENT_EMERALD
        };
        painter.rect(
            btn_rect,
            CornerRadius::same(7),
            btn_fill,
            Stroke::new(1.2, BORDER_DARK_CONTOUR),
            StrokeKind::Inside,
        );
        let btn_painter =
            painter.with_clip_rect(ui.clip_rect().intersect(btn_rect.shrink2(vec2(4.0, 1.0))));
        btn_painter.text(
            btn_rect.center(),
            Align2::CENTER_CENTER,
            app.i18n.tr("Chạy Game Ngay", "Launch Game Now"),
            FontId::proportional(12.5),
            Color32::WHITE,
        );
        if btn_resp.clicked() {
            app.click_sfx();
            app.launch_selected_profile(None);
        }
    }
}

fn process_live_touch_and_controls(
    app: &mut LauncherAppState,
    ui: &mut egui::Ui,
    canvas: Rect,
    reserved_rects: &[Rect],
    session: Option<&MobileSessionHandle>,
    virtual_mouse_active: bool,
    fb_size: (f32, f32),
) {
    let menu_rect_guard = floating_menu_hit_rect(&app.mobile_game, canvas);
    let controls_visible = !app.mobile_game.controls_hidden;
    let sensitivity = app.mobile_controls.mouse_sensitivity.clamp(0.25, 4.0);

    let mut touch_events = Vec::new();
    let mut primary_down = false;
    let mut primary_pressed = false;
    let mut primary_released = false;
    let mut primary_pos = None;

    ui.ctx().input(|i| {
        for ev in &i.events {
            if let egui::Event::Touch { id, phase, pos, .. } = ev {
                touch_events.push((id.0, *phase, *pos));
            }
        }
        primary_down = i.pointer.primary_down();
        primary_pressed = i.pointer.primary_pressed();
        primary_released = i.pointer.primary_released();
        primary_pos = i.pointer.interact_pos();
    });

    let synthetic_mouse_id: u64 = u64::MAX - 7;
    if touch_events.is_empty() {
        if let Some(pos) = primary_pos {
            if primary_pressed {
                app.mobile_game.pointer_fallback_active = true;
                touch_events.push((synthetic_mouse_id, TouchPhase::Start, pos));
            } else if primary_down && app.mobile_game.pointer_fallback_active {
                touch_events.push((synthetic_mouse_id, TouchPhase::Move, pos));
            }
        }
        if primary_released && app.mobile_game.pointer_fallback_active {
            app.mobile_game.pointer_fallback_active = false;
            let release_pos = primary_pos.unwrap_or(canvas.center());
            touch_events.push((synthetic_mouse_id, TouchPhase::End, release_pos));
        }
    }

    let mut tap_left_click = false;
    let mut tap_right_click = false;

    for (tid, phase, pos) in touch_events {
        match phase {
            TouchPhase::Start => {
                if menu_rect_guard.contains(pos) || reserved_rects.iter().any(|r| r.contains(pos)) {
                    continue;
                }
                if controls_visible {
                    if let Some(hit_idx) =
                        hit_test_control(pos, &app.mobile_controls.buttons, canvas)
                    {
                        app.mobile_game.active_touches.insert(
                            tid,
                            TouchRole::Control {
                                button_idx: hit_idx,
                            },
                        );
                        continue;
                    }
                }
                app.mobile_game.active_touches.insert(
                    tid,
                    TouchRole::Trackpad {
                        start_pos: pos,
                        last_pos: pos,
                        started_at: Instant::now(),
                        hold_triggered: false,
                        holding_left_break: false,
                    },
                );
            }
            TouchPhase::Move => {
                if let Some(role) = app.mobile_game.active_touches.get_mut(&tid) {
                    match role {
                        TouchRole::Control { button_idx } => {
                            let is_dpad = app
                                .mobile_controls
                                .buttons
                                .get(*button_idx)
                                .is_some_and(|b| b.shape == ControlShape::Dpad);
                            if !is_dpad {
                                if let Some(new_idx) =
                                    hit_test_control(pos, &app.mobile_controls.buttons, canvas)
                                {
                                    *button_idx = new_idx;
                                }
                            }
                        }
                        TouchRole::Trackpad { last_pos, .. } => {
                            let delta = pos - *last_pos;
                            *last_pos = pos;
                            if delta.length_sq() > 0.01 {
                                if virtual_mouse_active {
                                    let nx = (app.mobile_game.cursor_norm.0
                                        + (delta.x / canvas.width().max(1.0)) * sensitivity)
                                        .clamp(0.0, 1.0);
                                    let ny = (app.mobile_game.cursor_norm.1
                                        + (delta.y / canvas.height().max(1.0)) * sensitivity)
                                        .clamp(0.0, 1.0);
                                    app.mobile_game.cursor_norm = (nx, ny);
                                    if let Some(sess) = session {
                                        sess.send(MobileInputEvent::CursorPos {
                                            x: nx * fb_size.0,
                                            y: ny * fb_size.1,
                                        });
                                    }
                                } else {
                                    let scale_x =
                                        (fb_size.0 / canvas.width().max(1.0)) * sensitivity;
                                    let scale_y =
                                        (fb_size.1 / canvas.height().max(1.0)) * sensitivity;
                                    app.mobile_game.camera_accum_px.0 += delta.x * scale_x;
                                    app.mobile_game.camera_accum_px.1 += delta.y * scale_y;
                                    if let Some(sess) = session {
                                        sess.send(MobileInputEvent::CursorPos {
                                            x: app.mobile_game.camera_accum_px.0,
                                            y: app.mobile_game.camera_accum_px.1,
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            }
            TouchPhase::End | TouchPhase::Cancel => {
                if let Some(TouchRole::Trackpad {
                    start_pos,
                    started_at,
                    hold_triggered,
                    ..
                }) = app.mobile_game.active_touches.remove(&tid)
                {
                    let travel = start_pos.distance(pos);
                    let elapsed_ms = started_at.elapsed().as_millis();
                    if phase == TouchPhase::End
                        && !hold_triggered
                        && travel <= 10.0
                        && elapsed_ms <= 220
                    {
                        if virtual_mouse_active {
                            tap_left_click = true;
                        } else {
                            tap_right_click = true;
                        }
                    }
                }
            }
        }
    }

    let mut trackpad_holding_left = false;
    for role in app.mobile_game.active_touches.values_mut() {
        if let TouchRole::Trackpad {
            start_pos,
            last_pos,
            started_at,
            hold_triggered,
            holding_left_break,
        } = role
        {
            if !*hold_triggered
                && start_pos.distance(*last_pos) <= 10.0
                && started_at.elapsed().as_millis() >= 320
            {
                *hold_triggered = true;
                if virtual_mouse_active {
                    tap_right_click = true;
                } else {
                    *holding_left_break = true;
                }
            }
            if *holding_left_break {
                trackpad_holding_left = true;
            }
        }
    }

    let mut active_touch_positions_by_btn: HashMap<usize, Vec<Pos2>> = HashMap::new();
    let current_touch_pos_map: HashMap<u64, Pos2> = ui.ctx().input(|i| {
        let mut m = HashMap::new();
        for ev in &i.events {
            if let egui::Event::Touch { id, pos, .. } = ev {
                m.insert(id.0, *pos);
            }
        }
        if let Some(p) = i.pointer.interact_pos() {
            m.insert(synthetic_mouse_id, p);
        }
        m
    });

    for (tid, role) in &app.mobile_game.active_touches {
        if let TouchRole::Control { button_idx } = role {
            if let Some(&p) = current_touch_pos_map.get(tid) {
                active_touch_positions_by_btn
                    .entry(*button_idx)
                    .or_default()
                    .push(p);
            } else if let Some(btn) = app.mobile_controls.buttons.get(*button_idx) {
                active_touch_positions_by_btn
                    .entry(*button_idx)
                    .or_default()
                    .push(button_rect_on_canvas(btn, canvas).center());
            }
        }
    }

    let mut next_keys = HashSet::new();
    let mut next_mouse = HashSet::new();
    let mut active_dpad_cells = HashSet::new();
    let mut currently_touched_toggle_ids = HashSet::new();
    let mut currently_touched_action_ids = HashSet::new();
    let mut crouch_touched_now = false;
    let mut trigger_chat = false;
    let mut trigger_vmouse = false;
    let mut scroll_delta_y = 0.0_f64;

    if controls_visible {
        for (idx, btn) in app.mobile_controls.buttons.iter().enumerate() {
            let rect = button_rect_on_canvas(btn, canvas);
            let touches = active_touch_positions_by_btn.get(&idx);
            let is_touched = touches.is_some_and(|v| !v.is_empty());

            if btn.shape == ControlShape::Dpad {
                if let Some(pts) = touches {
                    for &pt in pts {
                        if let Some((r, c)) = dpad_cell_at(pt, rect) {
                            active_dpad_cells.insert((r, c));
                            match (r, c) {
                                (0, 0) => {
                                    next_keys.insert(GLFW_KEY_W);
                                    next_keys.insert(GLFW_KEY_A);
                                }
                                (0, 1) => {
                                    next_keys.insert(GLFW_KEY_W);
                                }
                                (0, 2) => {
                                    next_keys.insert(GLFW_KEY_W);
                                    next_keys.insert(GLFW_KEY_D);
                                }
                                (1, 0) => {
                                    next_keys.insert(GLFW_KEY_A);
                                }
                                (1, 1) => {
                                    crouch_touched_now = true;
                                }
                                (1, 2) => {
                                    next_keys.insert(GLFW_KEY_D);
                                }
                                (2, 0) => {
                                    next_keys.insert(GLFW_KEY_S);
                                    next_keys.insert(GLFW_KEY_A);
                                }
                                (2, 1) => {
                                    next_keys.insert(GLFW_KEY_S);
                                }
                                (2, 2) => {
                                    next_keys.insert(GLFW_KEY_S);
                                    next_keys.insert(GLFW_KEY_D);
                                }
                                _ => {}
                            }
                        }
                    }
                }
                continue;
            }

            let effective_active = if btn.toggle_mode {
                if is_touched {
                    currently_touched_toggle_ids.insert(btn.id.clone());
                    if !app.mobile_game.toggle_press_latches.contains(&btn.id) {
                        app.mobile_game.toggle_press_latches.insert(btn.id.clone());
                        if !app.mobile_game.toggled_button_ids.remove(&btn.id) {
                            app.mobile_game.toggled_button_ids.insert(btn.id.clone());
                        }
                    }
                }
                app.mobile_game.toggled_button_ids.contains(&btn.id)
            } else {
                is_touched
            };

            if is_touched {
                currently_touched_action_ids.insert(btn.id.clone());
            }

            match btn.binding {
                ControlBinding::Key { primary, secondary } => {
                    if effective_active {
                        if primary > 0 {
                            next_keys.insert(primary);
                        }
                        if secondary > 0 {
                            next_keys.insert(secondary);
                        }
                    }
                }
                ControlBinding::MouseButton { button } => {
                    if effective_active {
                        next_mouse.insert(button);
                    }
                }
                ControlBinding::ScrollUp => {
                    if is_touched && !app.mobile_game.toggle_press_latches.contains(&btn.id) {
                        app.mobile_game.toggle_press_latches.insert(btn.id.clone());
                        scroll_delta_y += 1.0;
                    }
                    if is_touched {
                        currently_touched_toggle_ids.insert(btn.id.clone());
                    }
                }
                ControlBinding::ScrollDown => {
                    if is_touched && !app.mobile_game.toggle_press_latches.contains(&btn.id) {
                        app.mobile_game.toggle_press_latches.insert(btn.id.clone());
                        scroll_delta_y -= 1.0;
                    }
                    if is_touched {
                        currently_touched_toggle_ids.insert(btn.id.clone());
                    }
                }
                ControlBinding::OpenChat => {
                    if is_touched && !app.mobile_game.toggle_press_latches.contains(&btn.id) {
                        app.mobile_game.toggle_press_latches.insert(btn.id.clone());
                        trigger_chat = true;
                    }
                    if is_touched {
                        currently_touched_toggle_ids.insert(btn.id.clone());
                    }
                }
                ControlBinding::ToggleVirtualMouse => {
                    if is_touched && !app.mobile_game.toggle_press_latches.contains(&btn.id) {
                        app.mobile_game.toggle_press_latches.insert(btn.id.clone());
                        trigger_vmouse = true;
                    }
                    if is_touched {
                        currently_touched_toggle_ids.insert(btn.id.clone());
                    }
                }
            }
        }
    }

    app.mobile_game
        .toggle_press_latches
        .retain(|id| currently_touched_toggle_ids.contains(id));

    if crouch_touched_now {
        if !app.mobile_game.crouch_touch_latch {
            app.mobile_game.crouch_touch_latch = true;
            app.mobile_game.crouch_toggled = !app.mobile_game.crouch_toggled;
        }
    } else {
        app.mobile_game.crouch_touch_latch = false;
    }
    if app.mobile_game.crouch_toggled {
        next_keys.insert(GLFW_KEY_LEFT_SHIFT);
    }

    if trackpad_holding_left || app.mobile_game.mouse_drag_lock {
        next_mouse.insert(GLFW_MOUSE_BUTTON_LEFT);
    }

    if trigger_chat {
        app.click_sfx();
        app.mobile_game.chat_bar_open = !app.mobile_game.chat_bar_open;
        if app.mobile_game.chat_bar_open && !virtual_mouse_active {
            if let Some(sess) = session {
                sess.send(MobileInputEvent::Key {
                    keycode: GLFW_KEY_T,
                    down: true,
                });
                app.mobile_game.pending_release_keys.push(GLFW_KEY_T);
            }
        }
    }
    if trigger_vmouse {
        app.click_sfx();
        app.mobile_game.virtual_mouse_forced = !app.mobile_game.virtual_mouse_forced;
    }

    if let Some(sess) = session {
        for &old_k in &app.mobile_game.prev_held_keys {
            if !next_keys.contains(&old_k) {
                sess.send(MobileInputEvent::Key {
                    keycode: old_k,
                    down: false,
                });
            }
        }
        for &new_k in &next_keys {
            if !app.mobile_game.prev_held_keys.contains(&new_k) {
                sess.send(MobileInputEvent::Key {
                    keycode: new_k,
                    down: true,
                });
            }
        }

        for &old_m in &app.mobile_game.prev_held_mouse {
            if !next_mouse.contains(&old_m) {
                sess.send(MobileInputEvent::MouseButton {
                    button: old_m,
                    down: false,
                });
            }
        }
        for &new_m in &next_mouse {
            if !app.mobile_game.prev_held_mouse.contains(&new_m) {
                sess.send(MobileInputEvent::MouseButton {
                    button: new_m,
                    down: true,
                });
            }
        }

        if tap_left_click && !next_mouse.contains(&GLFW_MOUSE_BUTTON_LEFT) {
            sess.send(MobileInputEvent::MouseButton {
                button: GLFW_MOUSE_BUTTON_LEFT,
                down: true,
            });
            app.mobile_game
                .pending_release_mouse
                .push(GLFW_MOUSE_BUTTON_LEFT);
        }
        if tap_right_click && !next_mouse.contains(&GLFW_MOUSE_BUTTON_RIGHT) {
            sess.send(MobileInputEvent::MouseButton {
                button: GLFW_MOUSE_BUTTON_RIGHT,
                down: true,
            });
            app.mobile_game
                .pending_release_mouse
                .push(GLFW_MOUSE_BUTTON_RIGHT);
        }
        if scroll_delta_y.abs() > 0.01 {
            sess.send(MobileInputEvent::Scroll {
                dx: 0.0,
                dy: scroll_delta_y,
            });
        }
    }

    app.mobile_game.prev_held_keys = next_keys;
    app.mobile_game.prev_held_mouse = next_mouse;

    if controls_visible {
        let primary_accent = accent_palette(app.settings.accent_preset).0;
        for btn in &app.mobile_controls.buttons {
            let rect = button_rect_on_canvas(btn, canvas);
            if btn.shape == ControlShape::Dpad {
                paint_bedrock_dpad(
                    ui,
                    rect,
                    btn.opacity,
                    &active_dpad_cells,
                    app.mobile_game.crouch_toggled,
                    primary_accent,
                    false,
                );
            } else {
                let is_active = currently_touched_action_ids.contains(&btn.id)
                    || app.mobile_game.toggled_button_ids.contains(&btn.id)
                    || (matches!(btn.binding, ControlBinding::ToggleVirtualMouse)
                        && app.mobile_game.virtual_mouse_forced)
                    || (matches!(btn.binding, ControlBinding::OpenChat)
                        && app.mobile_game.chat_bar_open);
                paint_bedrock_button(ui, btn, rect, is_active, false, primary_accent);
            }
        }
    }
}

fn paint_bedrock_dpad(
    ui: &mut egui::Ui,
    rect: Rect,
    opacity: f32,
    active_cells: &HashSet<(usize, usize)>,
    crouch_toggled: bool,
    accent: Color32,
    selected_in_editor: bool,
) {
    let painter = ui.painter();
    let alpha = (opacity.clamp(0.15, 1.0) * 185.0) as u8;
    let cell_w = rect.width() / 3.0;
    let cell_h = rect.height() / 3.0;
    let gap = 2.0;

    for r in 0..3 {
        for c in 0..3 {
            let is_diag = (r == 0 || r == 2) && (c == 0 || c == 2);
            let is_active = active_cells.contains(&(r, c)) || (r == 1 && c == 1 && crouch_toggled);
            if is_diag && !is_active && !selected_in_editor {
                continue;
            }
            let cell_rect = Rect::from_min_size(
                pos2(
                    rect.left() + (c as f32) * cell_w,
                    rect.top() + (r as f32) * cell_h,
                ),
                vec2(cell_w, cell_h),
            )
            .shrink(gap);

            let fill = if is_active {
                Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 215)
            } else if is_diag {
                Color32::from_rgba_unmultiplied(20, 28, 44, alpha / 2)
            } else {
                Color32::from_rgba_unmultiplied(18, 25, 38, alpha)
            };
            let border = if is_active {
                Stroke::new(1.8, Color32::WHITE)
            } else {
                Stroke::new(
                    1.2,
                    Color32::from_rgba_unmultiplied(190, 210, 235, alpha.saturating_add(30)),
                )
            };

            painter.rect(
                cell_rect,
                CornerRadius::same(8),
                fill,
                border,
                StrokeKind::Inside,
            );
            let fg = if is_active {
                Color32::from_rgb(8, 16, 14)
            } else {
                Color32::WHITE
            };
            paint_dpad_cell_symbol(painter, cell_rect, r, c, fg);
        }
    }

    if selected_in_editor {
        painter.rect_stroke(
            rect.expand(2.0),
            CornerRadius::same(10),
            Stroke::new(2.2, ACCENT_AMBER),
            StrokeKind::Outside,
        );
    }
}

fn paint_dpad_cell_symbol(
    painter: &egui::Painter,
    cell: Rect,
    row: usize,
    col: usize,
    color: Color32,
) {
    let center = cell.center();
    let rad = (cell.width().min(cell.height()) * 0.23).clamp(4.0, 11.0);
    if row == 1 && col == 1 {
        painter.circle_filled(center, rad * 0.72, color);
        return;
    }
    let dx = (col as f32) - 1.0;
    let dy = (row as f32) - 1.0;
    let dir = vec2(dx, dy).normalized();
    let perp = vec2(-dir.y, dir.x);
    let tip = center + dir * rad;
    let base_l = center - dir * (rad * 0.68) + perp * (rad * 0.78);
    let base_r = center - dir * (rad * 0.68) - perp * (rad * 0.78);
    painter.add(egui::Shape::convex_polygon(
        vec![tip, base_l, base_r],
        color,
        Stroke::NONE,
    ));
}

fn paint_bedrock_button(
    ui: &mut egui::Ui,
    btn: &MobileControlButton,
    rect: Rect,
    is_active: bool,
    selected_in_editor: bool,
    accent: Color32,
) {
    let painter = ui.painter();
    let alpha = (btn.opacity.clamp(0.15, 1.0) * 190.0) as u8;
    let fill = if is_active {
        Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 220)
    } else {
        Color32::from_rgba_unmultiplied(18, 25, 38, alpha)
    };
    let stroke = if selected_in_editor {
        Stroke::new(2.2, ACCENT_AMBER)
    } else if is_active {
        Stroke::new(1.8, Color32::WHITE)
    } else {
        Stroke::new(
            1.2,
            Color32::from_rgba_unmultiplied(185, 205, 230, alpha.saturating_add(35)),
        )
    };

    match btn.shape {
        ControlShape::Round => {
            let radius = rect.width().min(rect.height()) * 0.5;
            painter.circle(rect.center(), radius, fill, stroke);
        }
        ControlShape::Rect | ControlShape::Dpad => {
            painter.rect(
                rect,
                CornerRadius::same(7),
                fill,
                stroke,
                StrokeKind::Inside,
            );
        }
    }

    let font_sz = (rect.height() * 0.34).clamp(10.5, 15.5);
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        &btn.label,
        FontId::proportional(font_sz),
        if is_active {
            Color32::from_rgb(8, 16, 14)
        } else {
            Color32::WHITE
        },
    );
}

fn paint_virtual_mouse_cursor(ui: &mut egui::Ui, canvas: Rect, cursor_norm: (f32, f32)) {
    let painter = ui.painter();
    let tip = pos2(
        canvas.left() + cursor_norm.0.clamp(0.0, 1.0) * canvas.width(),
        canvas.top() + cursor_norm.1.clamp(0.0, 1.0) * canvas.height(),
    );
    let pts = vec![
        tip,
        pos2(tip.x, tip.y + 18.0),
        pos2(tip.x + 5.0, tip.y + 14.0),
        pos2(tip.x + 9.5, tip.y + 21.0),
        pos2(tip.x + 12.5, tip.y + 19.2),
        pos2(tip.x + 8.0, tip.y + 12.2),
        pos2(tip.x + 14.0, tip.y + 12.2),
    ];
    painter.add(egui::Shape::convex_polygon(
        pts,
        Color32::WHITE,
        Stroke::new(1.5, Color32::from_rgb(10, 16, 26)),
    ));
}

fn virtual_mouse_dock_rect(canvas: Rect) -> Rect {
    let w = 258.0_f32.min((canvas.width() - 12.0).max(112.0));
    let x = (canvas.left() + 10.0).min((canvas.right() - w - 4.0).max(canvas.left() + 4.0));
    let y = (canvas.top() + 48.0).min((canvas.bottom() - 38.0).max(canvas.top() + 4.0));
    Rect::from_min_size(pos2(x, y), vec2(w, 34.0))
}

fn render_virtual_mouse_dock(
    app: &mut LauncherAppState,
    ctx: &egui::Context,
    canvas: Rect,
    session: Option<&MobileSessionHandle>,
    fb_size: (f32, f32),
) {
    let dock_rect = virtual_mouse_dock_rect(canvas);
    let max_dock_w = (canvas.width() - 16.0).max(108.0);
    egui::Area::new(Id::unique("thk_virtual_mouse_dock"))
        .order(Order::Foreground)
        .fixed_pos(dock_rect.min)
        .show(ctx, |ui| {
            let dark = ui.visuals().dark_mode;
            let (fill, stroke, hold_idle_col) = if dark {
                (
                    Color32::from_rgba_unmultiplied(13, 19, 31, 218),
                    Stroke::new(1.0, Color32::from_rgba_unmultiplied(56, 189, 248, 145)),
                    Color32::WHITE,
                )
            } else {
                (
                    Color32::from_rgba_unmultiplied(245, 250, 255, 238),
                    Stroke::new(1.5, BORDER_DARK_CONTOUR),
                    TEXT_INK_LIGHT,
                )
            };
            Frame::NONE
                .fill(fill)
                .stroke(stroke)
                .corner_radius(CornerRadius::same(8))
                .inner_margin(Margin::symmetric(8, 4))
                .show(ui, |ui| {
                    ui.set_max_width(max_dock_w);
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = vec2(5.0, 3.0);
                        if ui
                            .button(
                                RichText::new(app.i18n.tr("Trái", "Left"))
                                    .size(11.5)
                                    .strong(),
                            )
                            .clicked()
                        {
                            if let Some(sess) = session {
                                sess.send(MobileInputEvent::CursorPos {
                                    x: app.mobile_game.cursor_norm.0 * fb_size.0,
                                    y: app.mobile_game.cursor_norm.1 * fb_size.1,
                                });
                                sess.send(MobileInputEvent::MouseButton {
                                    button: GLFW_MOUSE_BUTTON_LEFT,
                                    down: true,
                                });
                                app.mobile_game
                                    .pending_release_mouse
                                    .push(GLFW_MOUSE_BUTTON_LEFT);
                            }
                        }
                        if ui
                            .button(
                                RichText::new(app.i18n.tr("Phải", "Right"))
                                    .size(11.5)
                                    .strong(),
                            )
                            .clicked()
                        {
                            if let Some(sess) = session {
                                sess.send(MobileInputEvent::CursorPos {
                                    x: app.mobile_game.cursor_norm.0 * fb_size.0,
                                    y: app.mobile_game.cursor_norm.1 * fb_size.1,
                                });
                                sess.send(MobileInputEvent::MouseButton {
                                    button: GLFW_MOUSE_BUTTON_RIGHT,
                                    down: true,
                                });
                                app.mobile_game
                                    .pending_release_mouse
                                    .push(GLFW_MOUSE_BUTTON_RIGHT);
                            }
                        }
                        let drag_lbl = RichText::new(app.i18n.tr("Giữ Kéo", "Hold"))
                            .size(11.5)
                            .strong()
                            .color(if app.mobile_game.mouse_drag_lock {
                                readable_accent_ink(ACCENT_EMERALD, dark)
                            } else {
                                hold_idle_col
                            });
                        if ui
                            .selectable_label(app.mobile_game.mouse_drag_lock, drag_lbl)
                            .clicked()
                        {
                            app.mobile_game.mouse_drag_lock = !app.mobile_game.mouse_drag_lock;
                        }
                        if ui
                            .button(RichText::new(app.i18n.tr("Lên", "Up")).size(11.5))
                            .clicked()
                        {
                            if let Some(sess) = session {
                                sess.send(MobileInputEvent::Scroll { dx: 0.0, dy: 1.0 });
                            }
                        }
                        if ui
                            .button(RichText::new(app.i18n.tr("Xuống", "Down")).size(11.5))
                            .clicked()
                        {
                            if let Some(sess) = session {
                                sess.send(MobileInputEvent::Scroll { dx: 0.0, dy: -1.0 });
                            }
                        }
                    });
                });
        });
}

fn render_edit_mode_canvas(app: &mut LauncherAppState, ui: &mut egui::Ui, canvas: Rect) {
    let painter = ui.painter();
    let guide_stroke = Stroke::new(1.0, Color32::from_rgba_unmultiplied(251, 191, 36, 38));
    for step in 1..10 {
        let t = (step as f32) * 0.1;
        let x = canvas.left() + canvas.width() * t;
        let y = canvas.top() + canvas.height() * t;
        painter.line_segment(
            [pos2(x, canvas.top()), pos2(x, canvas.bottom())],
            guide_stroke,
        );
        painter.line_segment(
            [pos2(canvas.left(), y), pos2(canvas.right(), y)],
            guide_stroke,
        );
    }

    let primary_accent = accent_palette(app.settings.accent_preset).0;
    let empty_cells = HashSet::new();
    let num_buttons = app.mobile_controls.buttons.len();

    for idx in 0..num_buttons {
        let rect = button_rect_on_canvas(&app.mobile_controls.buttons[idx], canvas);
        let resp = ui.interact(
            rect,
            ui.make_persistent_id(("thk_mobile_ctrl_edit_item", idx)),
            Sense::click_and_drag(),
        );
        if resp.clicked() || resp.drag_started() {
            app.mobile_game.selected_btn_idx = Some(idx);
        }
        if resp.dragged() {
            app.mobile_game.selected_btn_idx = Some(idx);
            let delta = resp.drag_delta();
            let btn = &mut app.mobile_controls.buttons[idx];
            btn.x_norm = (btn.x_norm + delta.x / canvas.width().max(1.0)).clamp(0.03, 0.97);
            btn.y_norm = (btn.y_norm + delta.y / canvas.height().max(1.0)).clamp(0.14, 0.96);
        }

        let is_selected = app.mobile_game.selected_btn_idx == Some(idx);
        let btn = &app.mobile_controls.buttons[idx];
        let updated_rect = button_rect_on_canvas(btn, canvas);
        if btn.shape == ControlShape::Dpad {
            paint_bedrock_dpad(
                ui,
                updated_rect,
                btn.opacity,
                &empty_cells,
                false,
                primary_accent,
                is_selected,
            );
        } else {
            paint_bedrock_button(ui, btn, updated_rect, false, is_selected, primary_accent);
        }
    }

    let ctx = ui.ctx().clone();
    egui::Area::new(Id::unique("thk_mobile_key_editor_toolbar"))
        .order(Order::Foreground)
        .fixed_pos(pos2(canvas.left() + 8.0, canvas.top() + 6.0))
        .show(&ctx, |ui| {
            let dark = ui.visuals().dark_mode;
            let fill = if dark {
                Color32::from_rgba_unmultiplied(12, 18, 30, 240)
            } else {
                Color32::from_rgba_unmultiplied(245, 250, 255, 244)
            };
            Frame::NONE
                .fill(fill)
                .stroke(Stroke::new(
                    1.4,
                    if dark {
                        ACCENT_AMBER
                    } else {
                        BORDER_DARK_CONTOUR
                    },
                ))
                .corner_radius(CornerRadius::same(10))
                .inner_margin(Margin::symmetric(10, 8))
                .show(ui, |ui| {
                    let toolbar_max_w = (canvas.width() - 36.0).max(120.0);
                    ui.set_max_width(toolbar_max_w);
                    ScrollArea::vertical()
                        .id_salt("mobile_key_editor_toolbar_scroll")
                        .max_height((canvas.height() * 0.42).max(64.0))
                        .auto_shrink([false, true])
                        .show(ui, |ui| {
                            ui.set_max_width(toolbar_max_w);
                            ui.horizontal_wrapped(|ui| {
                                ui.label(
                                    RichText::new(
                                        app.i18n.tr("Edit Phím Bedrock", "Edit Bedrock Controls"),
                                    )
                                    .color(readable_accent_ink(ACCENT_AMBER, dark))
                                    .strong(),
                                );

                                if ui
                                    .button(
                                        RichText::new(app.i18n.tr("+ Thêm Phím", "+ Add Key"))
                                            .color(readable_accent_ink(ACCENT_EMERALD, dark))
                                            .strong(),
                                    )
                                    .clicked()
                                {
                                    app.click_sfx();
                                    let new_btn = MobileControlButton {
                                        id: format!("custom_{}", Uuid::new_v4().simple()),
                                        label: "F3".into(),
                                        x_norm: 0.50,
                                        y_norm: 0.48,
                                        width_dp: 50.0,
                                        height_dp: 50.0,
                                        scale: 1.0,
                                        opacity: 0.80,
                                        toggle_mode: false,
                                        shape: ControlShape::Round,
                                        binding: thk_core::mobile_controls::ControlBinding::Key {
                                            primary: thk_core::mobile_controls::GLFW_KEY_F3,
                                            secondary: 0,
                                        },
                                    };
                                    app.mobile_controls.buttons.push(new_btn);
                                    app.mobile_game.selected_btn_idx =
                                        Some(app.mobile_controls.buttons.len() - 1);
                                }

                                if ui
                                    .button(app.i18n.tr("Mặc định", "Reset Default"))
                                    .clicked()
                                {
                                    app.click_sfx();
                                    app.mobile_controls = MobileControlLayout::bedrock_default();
                                    app.mobile_game.selected_btn_idx = Some(0);
                                }

                                if ui
                                    .button(
                                        RichText::new(app.i18n.tr("Lưu & Xong", "Save & Done"))
                                            .color(ACCENT_CYAN)
                                            .strong(),
                                    )
                                    .clicked()
                                {
                                    app.click_sfx();
                                    app.save_mobile_controls_with_toast();
                                    app.mobile_game.edit_mode = false;
                                }
                            });

                            if let Some(sel_idx) = app
                                .mobile_game
                                .selected_btn_idx
                                .filter(|&i| i < app.mobile_controls.buttons.len())
                            {
                                ui.separator();
                                let is_dpad = app.mobile_controls.buttons[sel_idx].shape
                                    == ControlShape::Dpad;
                                let can_delete =
                                    app.mobile_controls.buttons[sel_idx].id != "bedrock_dpad";
                                let mut delete_clicked = false;

                                ui.horizontal_wrapped(|ui| {
                                    let adaptive_sw = (toolbar_max_w - 64.0).clamp(48.0, 110.0);
                                    if ui.spacing().slider_width > adaptive_sw {
                                        ui.spacing_mut().slider_width = adaptive_sw;
                                    }
                                    let btn = &mut app.mobile_controls.buttons[sel_idx];
                                    if !is_dpad {
                                        let name_lbl = ui.label(app.i18n.tr("Nhãn:", "Label:"));
                                        ui.add(
                                            egui::TextEdit::singleline(&mut btn.label)
                                                .desired_width(
                                                    68.0_f32.min((toolbar_max_w - 48.0).max(42.0)),
                                                ),
                                        )
                                        .labelled_by(name_lbl.id);

                                        egui::ComboBox::from_id_salt("mobile_ctrl_binding_combo")
                                            .width(168.0_f32.min((toolbar_max_w - 20.0).max(84.0)))
                                            .truncate()
                                            .selected_text(btn.binding.display_label())
                                            .show_ui(ui, |ui| {
                                                for opt in bindable_catalog() {
                                                    if ui
                                                        .selectable_label(
                                                            btn.binding == opt.binding,
                                                            opt.label,
                                                        )
                                                        .clicked()
                                                    {
                                                        btn.binding = opt.binding;
                                                        btn.label = opt.short_name.to_string();
                                                    }
                                                }
                                            })
                                            .response
                                            .widget_info(|| {
                                                egui::WidgetInfo::labeled(
                                                    egui::Role::ComboBox,
                                                    ui.is_enabled(),
                                                    btn.binding.display_label(),
                                                )
                                            });

                                        ui.selectable_value(
                                            &mut btn.shape,
                                            ControlShape::Round,
                                            app.i18n.tr("Tròn", "Round"),
                                        );
                                        ui.selectable_value(
                                            &mut btn.shape,
                                            ControlShape::Rect,
                                            app.i18n.tr("Vuông", "Rect"),
                                        );
                                        ui.checkbox(
                                            &mut btn.toggle_mode,
                                            app.i18n.tr("Giữ phím", "Toggle"),
                                        );
                                    } else {
                                        ui.label(
                                            RichText::new(
                                                app.i18n.tr("Cụm phím di chuyển", "Movement D-Pad"),
                                            )
                                            .strong(),
                                        );
                                    }

                                    let size_lbl = ui.label(app.i18n.tr("Cỡ:", "Size:"));
                                    ui.add(
                                        egui::Slider::new(&mut btn.scale, 0.5..=2.2)
                                            .max_decimals(2)
                                            .suffix("x"),
                                    )
                                    .labelled_by(size_lbl.id);

                                    let opacity_lbl = ui.label(app.i18n.tr("Độ mờ:", "Opacity:"));
                                    let mut pct = (btn.opacity * 100.0).round();
                                    let opacity_resp = ui
                                        .add(egui::Slider::new(&mut pct, 15.0..=100.0).suffix("%"));
                                    let opacity_changed = opacity_resp.changed();
                                    opacity_resp.labelled_by(opacity_lbl.id);
                                    if opacity_changed {
                                        btn.opacity = (pct / 100.0).clamp(0.15, 1.0);
                                    }

                                    if can_delete
                                        && ui
                                            .button(
                                                RichText::new(app.i18n.tr("Xóa", "Delete"))
                                                    .color(ACCENT_RED),
                                            )
                                            .clicked()
                                    {
                                        delete_clicked = true;
                                    }
                                });

                                if delete_clicked {
                                    app.click_sfx();
                                    app.mobile_controls.buttons.remove(sel_idx);
                                    app.mobile_game.selected_btn_idx = None;
                                }
                            } else {
                                ui.label(
                                    RichText::new(app.i18n.tr(
                                        "Chạm vào phím để tuỳ chỉnh",
                                        "Tap a key to customize",
                                    ))
                                    .size(11.3)
                                    .weak(),
                                );
                            }
                        });
                });
        });
}

fn render_inline_chat_bar(
    app: &mut LauncherAppState,
    ctx: &egui::Context,
    canvas: Rect,
    session: Option<&MobileSessionHandle>,
) {
    let pos_x = if canvas.width() < 520.0 {
        canvas.left() + 8.0
    } else {
        canvas.left() + canvas.width() * 0.16
    };
    let max_bar_w = (canvas.width() - 16.0).max(116.0);
    let pos_y = (canvas.top() + 44.0).min((canvas.bottom() - 46.0).max(canvas.top() + 4.0));
    let pos = pos2(pos_x, pos_y);
    egui::Area::new(Id::unique("thk_mobile_inline_chat"))
        .order(Order::Foreground)
        .fixed_pos(pos)
        .show(ctx, |ui| {
            let dark = ui.visuals().dark_mode;
            let fill = if dark {
                Color32::from_rgba_unmultiplied(12, 18, 30, 236)
            } else {
                Color32::from_rgba_unmultiplied(245, 250, 255, 242)
            };
            Frame::NONE
                .fill(fill)
                .stroke(Stroke::new(
                    1.2,
                    if dark {
                        ACCENT_CYAN
                    } else {
                        BORDER_DARK_CONTOUR
                    },
                ))
                .corner_radius(CornerRadius::same(9))
                .inner_margin(Margin::symmetric(10, 6))
                .show(ui, |ui| {
                    ui.set_max_width(max_bar_w);
                    ui.horizontal_wrapped(|ui| {
                        let width = if canvas.width() < 520.0 {
                            (max_bar_w - 102.0).clamp(64.0, 320.0)
                        } else {
                            (canvas.width() * 0.46).clamp(140.0, 420.0)
                        };
                        let edit_resp = ui.add(
                            egui::TextEdit::singleline(&mut app.mobile_game.chat_draft)
                                .hint_text(
                                    app.i18n
                                        .tr("Nhập chat hoặc /lệnh...", "Type chat or /command..."),
                                )
                                .desired_width(width),
                        );
                        let enter_pressed =
                            edit_resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                        let send_clicked = ui
                            .button(
                                RichText::new(app.i18n.tr("Gửi", "Send"))
                                    .color(readable_accent_ink(ACCENT_EMERALD, dark))
                                    .strong(),
                            )
                            .clicked();

                        if (enter_pressed || send_clicked)
                            && !app.mobile_game.chat_draft.trim().is_empty()
                        {
                            let msg = std::mem::take(&mut app.mobile_game.chat_draft);
                            if let Some(sess) = session {
                                for ch in msg.chars() {
                                    sess.send(MobileInputEvent::Char { ch, mods: 0 });
                                }
                                sess.send(MobileInputEvent::Key {
                                    keycode: GLFW_KEY_ENTER,
                                    down: true,
                                });
                                app.mobile_game.pending_release_keys.push(GLFW_KEY_ENTER);
                            }
                            app.mobile_game.chat_bar_open = false;
                        }

                        if ui.button("X").clicked() {
                            app.mobile_game.chat_bar_open = false;
                        }
                    });
                });
        });
}

fn render_inline_log_overlay(app: &LauncherAppState, ctx: &egui::Context, canvas: Rect) {
    let max_w = (canvas.width() - 16.0).max(116.0);
    let max_h = (canvas.height() - 56.0).max(76.0);
    let box_size = vec2(
        (canvas.width() * 0.56).clamp(240.0_f32.min(max_w), 560.0_f32.min(max_w)),
        (canvas.height() * 0.36).clamp(130.0_f32.min(max_h), 240.0_f32.min(max_h)),
    );
    let pos = pos2(
        (canvas.right() - box_size.x - 8.0).max(canvas.left() + 8.0),
        (canvas.top() + 50.0).min((canvas.bottom() - box_size.y - 6.0).max(canvas.top() + 6.0)),
    );
    egui::Area::new(Id::unique("thk_mobile_inline_logs"))
        .order(Order::Foreground)
        .fixed_pos(pos)
        .show(ctx, |ui| {
            let dark = ui.visuals().dark_mode;
            let (fill, stroke, line_col) = if dark {
                (
                    Color32::from_rgba_unmultiplied(9, 13, 22, 224),
                    Stroke::new(1.0, ACCENT_BLUE),
                    Color32::from_rgb(206, 218, 235),
                )
            } else {
                (
                    Color32::from_rgba_unmultiplied(245, 250, 255, 240),
                    Stroke::new(1.4, BORDER_DARK_CONTOUR),
                    TEXT_INK_LIGHT,
                )
            };
            Frame::NONE
                .fill(fill)
                .stroke(stroke)
                .corner_radius(CornerRadius::same(9))
                .inner_margin(Margin::same(8))
                .show(ui, |ui| {
                    ui.set_min_size(box_size);
                    ui.set_max_size(box_size);
                    ui.label(
                        RichText::new(app.i18n.tr("Nhật ký phiên Game", "Live Session Logs"))
                            .size(11.8)
                            .color(readable_accent_ink(ACCENT_CYAN, dark))
                            .strong(),
                    );
                    ScrollArea::vertical()
                        .id_salt("mobile_inline_log_scroll")
                        .stick_to_bottom(true)
                        .show(ui, |ui| {
                            if let Some(lines) = app.home.instance_logs.values().last() {
                                let start = lines.len().saturating_sub(80);
                                for line in &lines[start..] {
                                    ui.label(
                                        RichText::new(line).monospace().size(10.2).color(line_col),
                                    );
                                }
                            } else {
                                ui.label(
                                    RichText::new(
                                        app.i18n.tr("Chưa có dòng nhật ký.", "No log lines yet."),
                                    )
                                    .size(11.0)
                                    .weak(),
                                );
                            }
                        });
                });
        });
}

pub const FLOATING_ORB_DIAMETER: f32 = 48.0;

pub(crate) fn floating_menu_hit_rect(state: &MobileGameUiState, screen: Rect) -> Rect {
    let default_pos = pos2(screen.left() + 10.0, screen.top() + 8.0);
    let mut orb_pos = state.floating_menu_pos.unwrap_or(default_pos);
    orb_pos.x = orb_pos.x.clamp(
        screen.left() + 4.0,
        (screen.right() - FLOATING_ORB_DIAMETER - 4.0).max(screen.left() + 4.0),
    );
    orb_pos.y = orb_pos.y.clamp(
        screen.top() + 4.0,
        (screen.bottom() - FLOATING_ORB_DIAMETER - 4.0).max(screen.top() + 4.0),
    );
    if state.floating_menu_expanded {
        let card_outer_w = 224.0_f32.min((screen.width() - 8.0).max(112.0));
        let full_h = 348.0_f32.min((screen.height() - 8.0).max(112.0));
        let draw_x = orb_pos.x.clamp(
            screen.left() + 4.0,
            (screen.right() - card_outer_w - 4.0).max(screen.left() + 4.0),
        );
        let draw_y = orb_pos.y.clamp(
            screen.top() + 4.0,
            (screen.bottom() - full_h - 4.0).max(screen.top() + 4.0),
        );
        Rect::from_min_size(pos2(draw_x, draw_y), vec2(card_outer_w, full_h))
    } else {
        Rect::from_min_size(orb_pos, vec2(FLOATING_ORB_DIAMETER, FLOATING_ORB_DIAMETER))
    }
}

pub fn render_mobile_floating_menu(app: &mut LauncherAppState, ctx: &egui::Context) {
    let screen = crate::views::safe_content_rect(ctx);
    let default_pos = pos2(screen.left() + 10.0, screen.top() + 8.0);
    let mut current_pos = app.mobile_game.floating_menu_pos.unwrap_or(default_pos);
    current_pos.x = current_pos.x.clamp(
        screen.left() + 4.0,
        (screen.right() - FLOATING_ORB_DIAMETER - 4.0).max(screen.left() + 4.0),
    );
    current_pos.y = current_pos.y.clamp(
        screen.top() + 4.0,
        (screen.bottom() - FLOATING_ORB_DIAMETER - 4.0).max(screen.top() + 4.0),
    );
    app.mobile_game.floating_menu_pos = Some(current_pos);

    let card_outer_w = 224.0_f32.min((screen.width() - 8.0).max(112.0));
    let popup_w = (card_outer_w - 20.0).max(92.0);
    let full_h = 348.0_f32.min((screen.height() - 8.0).max(112.0));
    let draw_pos = if app.mobile_game.floating_menu_expanded {
        pos2(
            current_pos.x.clamp(
                screen.left() + 4.0,
                (screen.right() - card_outer_w - 4.0).max(screen.left() + 4.0),
            ),
            current_pos.y.clamp(
                screen.top() + 4.0,
                (screen.bottom() - full_h - 4.0).max(screen.top() + 4.0),
            ),
        )
    } else {
        current_pos
    };

    let primary_accent = accent_palette(app.settings.accent_preset).0;
    let game_running = !app.instance_supervisor.running_keys().is_empty();

    egui::Area::new(Id::unique("thk_mobile_floating_hub_menu"))
        .order(Order::Tooltip)
        .fixed_pos(draw_pos)
        .show(ctx, |ui| {
            let dark = ui.visuals().dark_mode;
            let (menu_fill, menu_stroke) = if dark {
                (
                    Color32::from_rgba_unmultiplied(12, 18, 30, 236),
                    Stroke::new(1.6, primary_accent),
                )
            } else {
                (
                    Color32::from_rgba_unmultiplied(245, 250, 255, 246),
                    Stroke::new(1.8, BORDER_DARK_CONTOUR),
                )
            };
            let hub_btn_fg = if (primary_accent.r() as u16
                + primary_accent.g() as u16 * 2
                + primary_accent.b() as u16)
                > 520
            {
                Color32::from_rgb(12, 22, 18)
            } else {
                Color32::WHITE
            };

            ui.spacing_mut().item_spacing.y = 6.0;
            let (orb_rect, orb_resp) = ui.allocate_exact_size(
                vec2(FLOATING_ORB_DIAMETER, FLOATING_ORB_DIAMETER),
                Sense::click_and_drag(),
            );
            let quick_menu_lbl = app.i18n.tr("Menu nhanh", "Quick Menu").to_string();
            orb_resp.widget_info(move || {
                egui::WidgetInfo::labeled(egui::Role::Button, true, &quick_menu_lbl)
            });

            let orb_fill = if orb_resp.hovered() || app.mobile_game.floating_menu_expanded {
                if dark {
                    Color32::from_rgba_unmultiplied(20, 30, 48, 248)
                } else {
                    Color32::from_rgba_unmultiplied(252, 254, 255, 252)
                }
            } else {
                menu_fill
            };
            let center = orb_rect.center();
            let radius = FLOATING_ORB_DIAMETER * 0.5;
            {
                let painter = ui.painter();
                painter.circle_filled(
                    center + vec2(0.0, 2.5),
                    radius + 0.5,
                    Color32::from_black_alpha(if dark { 112 } else { 58 }),
                );
                painter.circle_filled(center, radius, orb_fill);
                painter.circle_stroke(center, radius - 0.9, menu_stroke);
            }
            let icon_size = 26.0;
            let icon_rect = Rect::from_center_size(center, vec2(icon_size, icon_size));
            let mut icon_ui = ui.new_child(egui::UiBuilder::new().max_rect(icon_rect));
            paint_icon_3d(
                &mut icon_ui,
                icon_size,
                Icon3dKind::BrandVoxelCore,
                game_running,
            );

            if orb_resp.dragged() {
                let next = current_pos + orb_resp.drag_delta();
                app.mobile_game.floating_menu_pos = Some(next);
            } else if orb_resp.clicked() {
                app.click_sfx();
                app.mobile_game.floating_menu_expanded = !app.mobile_game.floating_menu_expanded;
            }

            if app.mobile_game.floating_menu_expanded {
                Frame::NONE
                    .fill(menu_fill)
                    .stroke(menu_stroke)
                    .corner_radius(CornerRadius::same(14))
                    .inner_margin(Margin::symmetric(10, 8))
                    .show(ui, |ui| {
                        ui.set_min_width(popup_w);
                        ui.set_max_width(popup_w);
                        ScrollArea::vertical()
                            .id_salt("thk_hub_popup_scroll")
                            .max_height((full_h - FLOATING_ORB_DIAMETER - 26.0).max(52.0))
                            .auto_shrink([false, true])
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.vertical(|ui| {
                                    ui.spacing_mut().item_spacing.y = 5.0;

                                    let hub_lbl = if app.mobile_game.hub_visible_over_game {
                                        app.i18n.tr("Về Game", "Back to Game")
                                    } else {
                                        app.i18n.tr("Mở Launcher", "Open Launcher")
                                    };
                                    if ui
                                        .add_sized(
                                            [ui.available_width(), 28.0],
                                            egui::Button::new(
                                                RichText::new(hub_lbl).strong().color(hub_btn_fg),
                                            )
                                            .fill(primary_accent),
                                        )
                                        .clicked()
                                    {
                                        app.click_sfx();
                                        let session = app
                                            .instance_supervisor
                                            .active_mobile_session()
                                            .map(|(_, s)| s);
                                        app.mobile_game.reset_transient_inputs(session.as_ref());
                                        app.mobile_game.hub_visible_over_game =
                                            !app.mobile_game.hub_visible_over_game;
                                        app.mobile_game.floating_menu_expanded = false;
                                    }

                                    #[cfg(any(target_os = "android", test))]
                                    {
                                        let orient_short = app.mobile_orientation_label(
                                            app.settings.mobile_orientation,
                                        );
                                        let orient_lbl = format!(
                                            "{}: {orient_short}",
                                            app.i18n.tr("Xoay màn hình", "Orientation")
                                        );
                                        if ui
                                            .add_sized(
                                                [ui.available_width(), 26.0],
                                                egui::Button::new(orient_lbl),
                                            )
                                            .clicked()
                                        {
                                            app.click_sfx();
                                            app.cycle_mobile_orientation();
                                        }
                                    }

                                    if !app.mobile_game.hub_visible_over_game {
                                        let keys_lbl = if app.mobile_game.controls_hidden {
                                            app.i18n.tr("Hiện Phím Bedrock", "Show Bedrock Keys")
                                        } else {
                                            app.i18n.tr("Ẩn Phím Bedrock", "Hide Bedrock Keys")
                                        };
                                        if ui
                                            .add_sized(
                                                [ui.available_width(), 26.0],
                                                egui::Button::new(keys_lbl),
                                            )
                                            .clicked()
                                        {
                                            app.click_sfx();
                                            app.mobile_game.controls_hidden =
                                                !app.mobile_game.controls_hidden;
                                        }

                                        let mouse_lbl = if app.mobile_game.virtual_mouse_forced {
                                            app.i18n.tr(
                                                "Chuột Ảo: Luôn bật",
                                                "Virtual Mouse: Forced ON",
                                            )
                                        } else {
                                            app.i18n.tr("Chuột Ảo: Tự động", "Virtual Mouse: Auto")
                                        };
                                        if ui
                                            .add_sized(
                                                [ui.available_width(), 26.0],
                                                egui::Button::new(mouse_lbl),
                                            )
                                            .clicked()
                                        {
                                            app.click_sfx();
                                            app.mobile_game.virtual_mouse_forced =
                                                !app.mobile_game.virtual_mouse_forced;
                                        }

                                        ui.horizontal_wrapped(|ui| {
                                            let sw = (popup_w - 76.0).clamp(42.0, 118.0);
                                            if ui.spacing().slider_width > sw {
                                                ui.spacing_mut().slider_width = sw;
                                            }
                                            let sens_lbl = ui.label(
                                                RichText::new(app.i18n.tr("Độ nhạy:", "Sens:"))
                                                    .size(11.3),
                                            );
                                            let sens_resp = ui.add(
                                                egui::Slider::new(
                                                    &mut app.mobile_controls.mouse_sensitivity,
                                                    0.25..=4.0,
                                                )
                                                .max_decimals(2),
                                            );
                                            let stopped = sens_resp.drag_stopped();
                                            sens_resp.labelled_by(sens_lbl.id);
                                            if stopped {
                                                let _ = app.mobile_controls.save(&app.paths.root);
                                            }
                                        });

                                        let edit_lbl = if app.mobile_game.edit_mode {
                                            app.i18n.tr(
                                                "Lưu & Đóng Edit Phím",
                                                "Save & Exit Key Editor",
                                            )
                                        } else {
                                            app.i18n
                                                .tr("Edit Phím Tùy Chỉnh", "Edit Custom Controls")
                                        };
                                        if ui
                                            .add_sized(
                                                [ui.available_width(), 26.0],
                                                egui::Button::new(edit_lbl),
                                            )
                                            .clicked()
                                        {
                                            app.click_sfx();
                                            if app.mobile_game.edit_mode {
                                                app.save_mobile_controls_with_toast();
                                                app.mobile_game.edit_mode = false;
                                            } else {
                                                app.mobile_game.controls_hidden = false;
                                                app.mobile_game.edit_mode = true;
                                                app.mobile_game.selected_btn_idx = Some(0);
                                            }
                                            app.mobile_game.floating_menu_expanded = false;
                                        }

                                        if ui
                                            .add_sized(
                                                [ui.available_width(), 26.0],
                                                egui::Button::new(
                                                    app.i18n.tr(
                                                        "Khung Chat / Lệnh",
                                                        "Chat / Command Bar",
                                                    ),
                                                ),
                                            )
                                            .clicked()
                                        {
                                            app.click_sfx();
                                            app.mobile_game.chat_bar_open =
                                                !app.mobile_game.chat_bar_open;
                                            app.mobile_game.floating_menu_expanded = false;
                                        }

                                        if ui
                                            .add_sized(
                                                [ui.available_width(), 26.0],
                                                egui::Button::new(
                                                    app.i18n
                                                        .tr("Nhật ký Phiên Game", "Live Game Log"),
                                                ),
                                            )
                                            .clicked()
                                        {
                                            app.click_sfx();
                                            app.mobile_game.inline_log_open =
                                                !app.mobile_game.inline_log_open;
                                            app.mobile_game.floating_menu_expanded = false;
                                        }
                                    }

                                    if game_running {
                                        if ui
                                            .add_sized(
                                                [ui.available_width(), 26.0],
                                                egui::Button::new(
                                                    RichText::new(
                                                        app.i18n.tr("Đóng Game", "Stop Game"),
                                                    )
                                                    .color(ACCENT_RED)
                                                    .strong(),
                                                ),
                                            )
                                            .clicked()
                                        {
                                            app.click_sfx();
                                            for key in app.instance_supervisor.running_keys() {
                                                app.instance_supervisor.stop_instance(&key);
                                            }
                                            app.mobile_game.floating_menu_expanded = false;
                                        }
                                    } else if ui
                                        .add_sized(
                                            [ui.available_width(), 26.0],
                                            egui::Button::new(
                                                app.i18n
                                                    .tr("Thoát Màn Hình Game", "Exit Game Screen"),
                                            ),
                                        )
                                        .clicked()
                                    {
                                        app.click_sfx();
                                        app.mobile_game.edit_mode = false;
                                        app.mobile_game.screen_open = false;
                                        app.mobile_game.hub_visible_over_game = false;
                                        app.mobile_game.floating_menu_expanded = false;
                                    }
                                });
                            });
                    });
            }
        });
}

impl LauncherAppState {
    pub fn save_mobile_controls_with_toast(&mut self) {
        match self.mobile_controls.save(&self.paths.root) {
            Ok(()) => {
                let msg = self
                    .i18n
                    .tr(
                        "Đã lưu bố cục phím điều khiển trên di động.",
                        "Saved custom mobile control layout.",
                    )
                    .to_string();
                self.push_toast(ToastKind::Success, msg);
            }
            Err(e) => self.push_toast(ToastKind::Error, e.to_string()),
        }
    }

    pub fn open_mobile_game_screen(&mut self, edit_immediately: bool) {
        self.mobile_game.screen_open = true;
        self.mobile_game.hub_visible_over_game = false;
        self.mobile_game.edit_mode = edit_immediately;
        if edit_immediately {
            self.mobile_game.controls_hidden = false;
            self.mobile_game.selected_btn_idx = Some(0);
        }
    }

    pub fn is_mobile_game_viewport_transparent(&self) -> bool {
        self.mobile_game.screen_open
            && !self.mobile_game.hub_visible_over_game
            && !self.instance_supervisor.running_keys().is_empty()
    }
}
