use std::time::{Duration, Instant};

use dziwak_core::interpolate_stroke;
use dziwak_core::ruler::ruler_ticks;
use eframe::egui;

use crate::state::{ActiveTool, DziwakApp, SelectionShape};

/// Rysuje prostokątny obrys maszerujących mrówek (przerywana linia czarno-biała).
pub fn draw_marching_ants_rect(painter: &egui::Painter, rect: egui::Rect, phase: f32) {
    if rect.width() <= 0.0 || rect.height() <= 0.0 {
        return;
    }

    let dash_len = 4.0_f32;
    let cycle = 8.0_f32;

    let tl = rect.left_top();
    let tr = rect.right_top();
    let br = rect.right_bottom();
    let bl = rect.left_bottom();

    let edges = [(tl, tr), (tr, br), (br, bl), (bl, tl)];
    let mut total_dist = phase;

    for (p_start, p_end) in edges {
        let dir = p_end - p_start;
        let edge_len = dir.length();
        if edge_len <= 0.0 {
            continue;
        }
        let norm = dir / edge_len;

        let mut curr_dist = 0.0_f32;
        while curr_dist < edge_len {
            let next_dist = (curr_dist + dash_len).min(edge_len);
            let s0 = p_start + norm * curr_dist;
            let s1 = p_start + norm * next_dist;

            let pos_in_cycle = (total_dist + curr_dist).rem_euclid(cycle);
            let color = if pos_in_cycle < dash_len {
                egui::Color32::BLACK
            } else {
                egui::Color32::WHITE
            };

            painter.line_segment([s0, s1], egui::Stroke::new(1.0_f32, color));
            curr_dist = next_dist;
        }
        total_dist += edge_len;
    }
}

/// Rysuje eliptyczny obrys maszerujących mrówek (przerywana linia czarno-biała).
pub fn draw_marching_ants_ellipse(painter: &egui::Painter, rect: egui::Rect, phase: f32) {
    if rect.width() <= 0.0 || rect.height() <= 0.0 {
        return;
    }

    let rx = rect.width() * 0.5;
    let ry = rect.height() * 0.5;
    let center = rect.center();

    let segments = 64;
    let dash_len = 4.0_f32;
    let cycle = 8.0_f32;

    let mut prev_pt = egui::pos2(center.x + rx, center.y);
    let mut total_dist = phase;

    for i in 1..=segments {
        let angle = 2.0 * std::f32::consts::PI * (i as f32) / (segments as f32);
        let curr_pt = egui::pos2(center.x + rx * angle.cos(), center.y + ry * angle.sin());

        let seg_vec = curr_pt - prev_pt;
        let seg_len = seg_vec.length();
        if seg_len > 0.0 {
            let pos_in_cycle = total_dist.rem_euclid(cycle);
            let color = if pos_in_cycle < dash_len {
                egui::Color32::BLACK
            } else {
                egui::Color32::WHITE
            };
            painter.line_segment([prev_pt, curr_pt], egui::Stroke::new(1.0_f32, color));
            total_dist += seg_len;
        }
        prev_pt = curr_pt;
    }
}

/// Renderuje płótno i obsługuje bezpośrednią interakcję w obszarze roboczym.
pub fn render_canvas(app: &mut DziwakApp, ctx: &egui::Context, should_fit_to_viewport: bool) {
    let canvas_bg = egui::Color32::from_rgb(0x26, 0x26, 0x26);
    egui::CentralPanel::default()
        .frame(egui::Frame::NONE.fill(canvas_bg))
        .show(ctx, |ui| {
            let full_rect = ui.available_rect_before_wrap();
            let ruler_thickness = 18.0_f32;

            // Prostokąty linijek i właściwego płótna
            let corner_rect = egui::Rect::from_min_size(
                full_rect.min,
                egui::vec2(ruler_thickness, ruler_thickness),
            );
            let top_ruler_rect = egui::Rect::from_min_max(
                egui::pos2(full_rect.left() + ruler_thickness, full_rect.top()),
                egui::pos2(full_rect.right(), full_rect.top() + ruler_thickness),
            );
            let left_ruler_rect = egui::Rect::from_min_max(
                egui::pos2(full_rect.left(), full_rect.top() + ruler_thickness),
                egui::pos2(full_rect.left() + ruler_thickness, full_rect.bottom()),
            );
            let canvas_viewport_rect = egui::Rect::from_min_max(
                egui::pos2(
                    full_rect.left() + ruler_thickness,
                    full_rect.top() + ruler_thickness,
                ),
                full_rect.max,
            );

            // Wycentruj płótno przy pierwszym uruchomieniu lub po wczytaniu nowego pliku
            // oraz po każdej zmianie rozmiaru okna, dopóki użytkownik sam nie zmienił widoku
            let viewport_valid =
                canvas_viewport_rect.width() > 0.0 && canvas_viewport_rect.height() > 0.0;
            let size_changed =
                (canvas_viewport_rect.size() - app.last_viewport_size).length() > 0.5;
            if viewport_valid {
                if should_fit_to_viewport {
                    app.auto_fit = true;
                }
                if !app.initialized || should_fit_to_viewport || (app.auto_fit && size_changed) {
                    app.fit_to_viewport(canvas_viewport_rect);
                    app.initialized = true;
                } else if app.center_requested {
                    app.center_canvas(canvas_viewport_rect);
                }
                app.center_requested = false;
                app.last_viewport_size = canvas_viewport_rect.size();
            }

            // Obszar interaktywny dla całego widoku
            let (response_rect, _response) =
                ui.allocate_exact_size(ui.available_size(), egui::Sense::hover());

            let (
                pointer_delta,
                hover_pos,
                is_middle_down,
                is_primary_down,
                is_primary_pressed,
                space_down,
                alt_down,
            ) = ctx.input(|i| {
                (
                    i.pointer.delta(),
                    i.pointer.hover_pos(),
                    i.pointer.button_down(egui::PointerButton::Middle),
                    i.pointer.button_down(egui::PointerButton::Primary),
                    i.pointer.button_pressed(egui::PointerButton::Primary),
                    i.key_down(egui::Key::Space),
                    i.modifiers.alt,
                )
            });

            let is_hovered = hover_pos.is_some_and(|pos| canvas_viewport_rect.contains(pos));
            app.cursor_canvas_pos = if is_hovered {
                hover_pos.map(|pos| app.transform.screen_to_canvas(pos))
            } else {
                None
            };

            // Obsługa przesuwania widoku (ŚPM lub Spacja + LPM, lub narzędzie Przesuwanie + LPM)
            let pan_active = is_middle_down
                || (space_down && is_primary_down)
                || (app.active_tool == ActiveTool::Pan && is_primary_down);

            if pan_active && (is_hovered || app.is_panning) {
                app.is_panning = true;
                app.transform.pan += pointer_delta;
                app.auto_fit = false;
            } else if !pan_active {
                app.is_panning = false;
            }

            // Obsługa pipety (narzędzie Pipeta lub Alt+klik w Pędzlu/Ołówku/Aerografie)
            let can_sample = app.filter_dialog.is_none()
                && (app.active_tool == ActiveTool::Eyedropper
                    || (matches!(
                        app.active_tool,
                        ActiveTool::Brush | ActiveTool::Pencil | ActiveTool::Airbrush
                    ) && alt_down))
                && !space_down
                && !is_middle_down;

            if can_sample && is_primary_down {
                if let Some(pos) = hover_pos {
                    if is_hovered {
                        let canvas_pos = app.transform.screen_to_canvas(pos);
                        app.sample_color_at(canvas_pos);
                        ctx.request_repaint();
                    }
                }
            }

            // Obsługa pędzla / ołówka / aerografu / gumki (LPM bez Spacji, ŚPM i bez Alt)
            let is_brush_like = matches!(
                app.active_tool,
                ActiveTool::Brush | ActiveTool::Pencil | ActiveTool::Airbrush | ActiveTool::Eraser
            );
            let can_paint = app.filter_dialog.is_none()
                && is_brush_like
                && !space_down
                && !is_middle_down
                && !alt_down;

            if can_paint {
                if is_primary_down && (is_hovered || app.is_painting) {
                    if let Some(pos) = hover_pos {
                        let canvas_pos = app.transform.screen_to_canvas(pos);
                        let p2 = (canvas_pos.x, canvas_pos.y);
                        let spacing = (app.brush_size * 0.15).max(1.0);

                        if !app.is_painting {
                            // Początek pociągnięcia: zapisujemy stan w historii raz na pociągnięcie
                            app.push_history();
                            app.is_painting = true;

                            // Na początku pociągnięcia carry = spacing, aby pierwszy punkt był w p2
                            app.stroke_carry = spacing;
                            app.apply_tool_dab(p2.0, p2.1);
                            app.last_stroke_pos = Some(p2);
                            if app.active_tool == ActiveTool::Airbrush {
                                app.last_airbrush_dab = Some(Instant::now());
                                ctx.request_repaint_after(Duration::from_millis(50));
                            }
                        } else {
                            if let Some(p1) = app.last_stroke_pos {
                                let mut points = Vec::new();
                                app.stroke_carry = interpolate_stroke(
                                    p1,
                                    p2,
                                    spacing,
                                    app.stroke_carry,
                                    &mut points,
                                );
                                for pt in points {
                                    app.apply_tool_dab(pt.0, pt.1);
                                }
                                app.last_stroke_pos = Some(p2);
                            }

                            if app.active_tool == ActiveTool::Airbrush {
                                let now = Instant::now();
                                let should_dab = match app.last_airbrush_dab {
                                    None => true,
                                    Some(t) => now.duration_since(t) >= Duration::from_millis(50),
                                };
                                if should_dab {
                                    app.apply_tool_dab(p2.0, p2.1);
                                    app.last_airbrush_dab = Some(now);
                                }
                                let elapsed = app
                                    .last_airbrush_dab
                                    .map(|t| now.duration_since(t))
                                    .unwrap_or_default();
                                let remaining = Duration::from_millis(50).saturating_sub(elapsed);
                                ctx.request_repaint_after(remaining);
                            }
                        }
                        ctx.request_repaint();
                    }
                } else if !is_primary_down && app.is_painting {
                    app.is_painting = false;
                    app.last_stroke_pos = None;
                    app.last_airbrush_dab = None;
                }
            } else if app.is_painting {
                app.is_painting = false;
                app.last_stroke_pos = None;
                app.last_airbrush_dab = None;
            }

            // Obsługa klonowania (C): Ctrl+klik = źródło, malowanie LPM kopiuje z przesunięciem (T8)
            let can_clone = app.filter_dialog.is_none()
                && app.active_tool == ActiveTool::Clone
                && !space_down
                && !is_middle_down
                && !alt_down;

            if can_clone {
                let is_ctrl_down = ctx.input(|i| i.modifiers.command);
                if is_ctrl_down && is_primary_pressed && is_hovered {
                    if let Some(pos) = hover_pos {
                        let canvas_pos = app.transform.screen_to_canvas(pos);
                        app.set_clone_source((canvas_pos.x, canvas_pos.y));
                        ctx.request_repaint();
                    }
                } else if !is_ctrl_down && app.clone_source.is_some() {
                    if is_primary_down && (is_hovered || app.is_painting) {
                        if let Some(pos) = hover_pos {
                            let canvas_pos = app.transform.screen_to_canvas(pos);
                            let p2 = (canvas_pos.x, canvas_pos.y);
                            let spacing = (app.brush_size * 0.15).max(1.0);

                            if !app.is_painting {
                                app.stroke_carry = spacing;
                                app.start_clone_stroke(p2);
                            } else if let Some(p1) = app.last_stroke_pos {
                                let mut points = Vec::new();
                                app.stroke_carry = interpolate_stroke(
                                    p1,
                                    p2,
                                    spacing,
                                    app.stroke_carry,
                                    &mut points,
                                );
                                for pt in points {
                                    app.apply_clone_dab(pt.0, pt.1);
                                }
                                app.last_stroke_pos = Some(p2);
                            }
                            ctx.request_repaint();
                        }
                    } else if !is_primary_down && app.is_painting {
                        app.finish_clone_stroke();
                    }
                } else if !is_primary_down && app.is_painting {
                    app.finish_clone_stroke();
                }
            } else if app.is_painting && app.active_tool == ActiveTool::Clone {
                app.finish_clone_stroke();
            }

            // Obsługa rozmazywania (Shift+S, T9)
            let can_smudge = app.filter_dialog.is_none()
                && app.active_tool == ActiveTool::Smudge
                && !space_down
                && !is_middle_down
                && !alt_down;

            if can_smudge {
                if is_primary_down && (is_hovered || app.is_painting) {
                    if let Some(pos) = hover_pos {
                        let canvas_pos = app.transform.screen_to_canvas(pos);
                        let p2 = (canvas_pos.x, canvas_pos.y);
                        let spacing = (app.brush_size * 0.15).max(1.0);

                        if !app.is_painting {
                            app.push_history();
                            app.is_painting = true;
                            app.stroke_carry = spacing;
                            app.last_stroke_pos = Some(p2);
                        } else if let Some(p1) = app.last_stroke_pos {
                            let mut points = Vec::new();
                            app.stroke_carry =
                                interpolate_stroke(p1, p2, spacing, app.stroke_carry, &mut points);
                            let mut prev = p1;
                            for pt in points {
                                app.apply_smudge_dab(pt.0, pt.1, prev.0, prev.1);
                                prev = pt;
                            }
                            app.last_stroke_pos = Some(p2);
                        }
                        ctx.request_repaint();
                    }
                } else if !is_primary_down && app.is_painting {
                    app.is_painting = false;
                    app.last_stroke_pos = None;
                }
            } else if app.is_painting && app.active_tool == ActiveTool::Smudge {
                app.is_painting = false;
                app.last_stroke_pos = None;
            }

            // Obsługa rozjaśniania/ściemniania (Shift+D) oraz rozmywania/wyostrzania (Shift+U)
            let is_dodge_or_blur = matches!(
                app.active_tool,
                ActiveTool::DodgeBurn | ActiveTool::BlurSharpen
            );
            let can_dodge_blur = app.filter_dialog.is_none()
                && is_dodge_or_blur
                && !space_down
                && !is_middle_down
                && !alt_down;

            if can_dodge_blur {
                let invert_mode = ctx.input(|i| i.modifiers.command);
                if is_primary_down && (is_hovered || app.is_painting) {
                    if let Some(pos) = hover_pos {
                        let canvas_pos = app.transform.screen_to_canvas(pos);
                        let p2 = (canvas_pos.x, canvas_pos.y);
                        let spacing = (app.brush_size * 0.15).max(1.0);

                        let apply_dab = |app: &mut DziwakApp, x: f32, y: f32| {
                            if app.active_tool == ActiveTool::DodgeBurn {
                                app.apply_dodge_burn_dab(x, y, invert_mode);
                            } else if app.active_tool == ActiveTool::BlurSharpen {
                                app.apply_blur_sharpen_dab(x, y, invert_mode);
                            }
                        };

                        if !app.is_painting {
                            app.push_history();
                            app.is_painting = true;
                            app.stroke_carry = spacing;
                            apply_dab(app, p2.0, p2.1);
                            app.last_stroke_pos = Some(p2);
                        } else if let Some(p1) = app.last_stroke_pos {
                            let mut points = Vec::new();
                            app.stroke_carry =
                                interpolate_stroke(p1, p2, spacing, app.stroke_carry, &mut points);
                            for pt in points {
                                apply_dab(app, pt.0, pt.1);
                            }
                            app.last_stroke_pos = Some(p2);
                        }
                        ctx.request_repaint();
                    }
                } else if !is_primary_down && app.is_painting {
                    app.is_painting = false;
                    app.last_stroke_pos = None;
                }
            } else if app.is_painting && is_dodge_or_blur {
                app.is_painting = false;
                app.last_stroke_pos = None;
            }

            // Obsługa wypełniania kubełkiem (pojedyncze kliknięcie LPM na płótnie)
            let can_bucket = app.filter_dialog.is_none()
                && app.active_tool == ActiveTool::Bucket
                && !space_down
                && !is_middle_down;

            // Różdżka: Shift = dodaj, Ctrl = odejmij, Shift+Ctrl = przetnij (jak w GIMP-ie)
            if app.filter_dialog.is_none()
                && app.active_tool == ActiveTool::MagicWand
                && !space_down
                && !is_middle_down
                && is_primary_pressed
                && is_hovered
            {
                if let Some(pos) = hover_pos {
                    let mods = ctx.input(|i| i.modifiers);
                    let mode = match (mods.shift, mods.command) {
                        (true, true) => dziwak_core::selection::SelectMode::Intersect,
                        (true, false) => dziwak_core::selection::SelectMode::Add,
                        (false, true) => dziwak_core::selection::SelectMode::Subtract,
                        (false, false) => dziwak_core::selection::SelectMode::Replace,
                    };
                    let canvas_pos = app.transform.screen_to_canvas(pos);
                    app.apply_magic_wand(canvas_pos.x, canvas_pos.y, mode);
                    ctx.request_repaint();
                }
            }

            // Zaznaczenie wg koloru: Shift = dodaj, Ctrl = odejmij, Shift+Ctrl = przetnij (T4)
            if app.filter_dialog.is_none()
                && app.active_tool == ActiveTool::SelectColor
                && !space_down
                && !is_middle_down
                && is_primary_pressed
                && is_hovered
            {
                if let Some(pos) = hover_pos {
                    let mods = ctx.input(|i| i.modifiers);
                    let mode = match (mods.shift, mods.command) {
                        (true, true) => dziwak_core::selection::SelectMode::Intersect,
                        (true, false) => dziwak_core::selection::SelectMode::Add,
                        (false, true) => dziwak_core::selection::SelectMode::Subtract,
                        (false, false) => dziwak_core::selection::SelectMode::Replace,
                    };
                    let canvas_pos = app.transform.screen_to_canvas(pos);
                    app.apply_select_by_color(canvas_pos.x, canvas_pos.y, mode);
                    ctx.request_repaint();
                }
            }

            if can_bucket && is_primary_pressed && is_hovered {
                if let Some(pos) = hover_pos {
                    let canvas_pos = app.transform.screen_to_canvas(pos);
                    app.apply_bucket_fill(canvas_pos.x, canvas_pos.y);
                    ctx.request_repaint();
                }
            }

            // Narzędzie Tekst (T10): kliknięcie na płótnie ustawia pozycję tekstu i otwiera okno
            if app.filter_dialog.is_none()
                && app.active_tool == ActiveTool::Text
                && !space_down
                && !is_middle_down
                && is_primary_pressed
                && is_hovered
            {
                if let Some(pos) = hover_pos {
                    let canvas_pos = app.transform.screen_to_canvas(pos);
                    let (x, y) = (canvas_pos.x.round() as i32, canvas_pos.y.round() as i32);
                    if let Some(dialog) = &mut app.text_dialog {
                        dialog.pos = (x, y);
                    } else {
                        app.open_text_dialog(x, y);
                    }
                    ctx.request_repaint();
                }
            }

            // Obsługa narzędzia Lupa (Z): klik = powiększ, Alt/Ctrl+klik = pomniejsz, przeciągnięcie = powiększ obszar
            let can_zoom_tool = app.filter_dialog.is_none()
                && app.active_tool == ActiveTool::Zoom
                && !space_down
                && !is_middle_down;

            if can_zoom_tool {
                let secondary_pressed =
                    ctx.input(|i| i.pointer.button_pressed(egui::PointerButton::Secondary));
                if secondary_pressed && is_hovered {
                    if let Some(pos) = hover_pos {
                        app.transform.zoom_around(pos, 1.0 / 1.5, 0.05, 64.0);
                        app.tile_renderer.set_zoom(app.transform.zoom);
                        app.auto_fit = false;
                        ctx.request_repaint();
                    }
                } else if is_primary_down && (is_hovered || app.zoom_drag_start.is_some()) {
                    if let Some(pos) = hover_pos {
                        if app.zoom_drag_start.is_none() {
                            app.zoom_drag_start = Some(pos);
                        }
                        app.zoom_drag_current = Some(pos);
                        ctx.request_repaint();
                    }
                } else if !is_primary_down && app.zoom_drag_start.is_some() {
                    let start = app.zoom_drag_start.take();
                    let curr = app.zoom_drag_current.take().or(start);
                    if let (Some(p1), Some(p2)) = (start, curr) {
                        let drag_dist = (p2 - p1).length();
                        if drag_dist >= 5.0 {
                            let c1 = app.transform.screen_to_canvas(p1);
                            let c2 = app.transform.screen_to_canvas(p2);
                            let min_x = c1.x.min(c2.x);
                            let max_x = c1.x.max(c2.x);
                            let min_y = c1.y.min(c2.y);
                            let max_y = c1.y.max(c2.y);
                            let rect = egui::Rect::from_min_max(
                                egui::pos2(min_x, min_y),
                                egui::pos2(max_x, max_y),
                            );
                            app.zoom_to_canvas_rect(rect, canvas_viewport_rect);
                        } else {
                            let zoom_out =
                                alt_down || ctx.input(|i| i.modifiers.command || i.modifiers.ctrl);
                            let factor = if zoom_out { 1.0 / 1.5 } else { 1.5 };
                            app.transform.zoom_around(p1, factor, 0.05, 64.0);
                            app.tile_renderer.set_zoom(app.transform.zoom);
                            app.auto_fit = false;
                        }
                        ctx.request_repaint();
                    }
                }
            } else if app.zoom_drag_start.is_some() {
                app.zoom_drag_start = None;
                app.zoom_drag_current = None;
            }

            // Obsługa narzędzia Miarka (Shift+M)
            let can_measure = app.filter_dialog.is_none()
                && app.active_tool == ActiveTool::Measure
                && !space_down
                && !is_middle_down;

            if can_measure {
                if is_primary_down && (is_hovered || app.is_measuring) {
                    if let Some(pos) = hover_pos {
                        let canvas_pos = app.transform.screen_to_canvas(pos);
                        if !app.is_measuring {
                            app.measure_start = Some(canvas_pos);
                            app.measure_end = Some(canvas_pos);
                            app.is_measuring = true;
                        } else {
                            app.measure_end = Some(canvas_pos);
                        }
                        ctx.request_repaint();
                    }
                } else if !is_primary_down && app.is_measuring {
                    app.is_measuring = false;
                    ctx.request_repaint();
                }
            } else if app.is_measuring {
                app.is_measuring = false;
            }

            // Narzędzia transformacji (Obrót, Skalowanie, Odbicie): kliknięcie otwiera dialog jeśli jest zamknięty
            if app.filter_dialog.is_none()
                && matches!(
                    app.active_tool,
                    ActiveTool::Rotate | ActiveTool::Scale | ActiveTool::Flip
                )
                && app.transform_dialog.is_none()
                && !space_down
                && !is_middle_down
                && is_primary_pressed
                && is_hovered
            {
                app.set_active_tool(app.active_tool);
                ctx.request_repaint();
            }

            // Obsługa narzędzia Gradient (przeciągnięcie a -> b)
            let can_gradient = app.filter_dialog.is_none()
                && app.active_tool == ActiveTool::Gradient
                && !space_down
                && !is_middle_down;

            if can_gradient {
                if is_primary_down && (is_hovered || app.gradient_drag_start.is_some()) {
                    if let Some(pos) = hover_pos {
                        let canvas_pos = app.transform.screen_to_canvas(pos);
                        if app.gradient_drag_start.is_none() {
                            app.gradient_drag_start = Some(canvas_pos);
                        }
                        app.gradient_drag_current = Some(canvas_pos);
                        ctx.request_repaint();
                    }
                } else if !is_primary_down && app.gradient_drag_start.is_some() {
                    if let (Some(start), Some(curr)) =
                        (app.gradient_drag_start, app.gradient_drag_current)
                    {
                        app.apply_linear_gradient_between(start, curr);
                    }
                    app.gradient_drag_start = None;
                    app.gradient_drag_current = None;
                    ctx.request_repaint();
                }
            } else if app.gradient_drag_start.is_some() {
                app.gradient_drag_start = None;
                app.gradient_drag_current = None;
            }

            // Obsługa narzędzi zaznaczania (Prostokąt / Elipsa)
            let can_select = app.filter_dialog.is_none()
                && (app.active_tool == ActiveTool::SelectRect
                    || app.active_tool == ActiveTool::SelectEllipse)
                && !space_down
                && !is_middle_down;

            if can_select {
                if is_primary_down && (is_hovered || app.selection_drag_start.is_some()) {
                    if let Some(pos) = hover_pos {
                        let canvas_pos = app.transform.screen_to_canvas(pos);
                        if app.selection_drag_start.is_none() {
                            app.selection_drag_start = Some(canvas_pos);
                        }
                        app.selection_drag_current = Some(canvas_pos);
                        ctx.request_repaint();
                    }
                } else if !is_primary_down && app.selection_drag_start.is_some() {
                    if let (Some(start), Some(curr)) =
                        (app.selection_drag_start, app.selection_drag_current)
                    {
                        let shift_down = ctx.input(|i| i.modifiers.shift);
                        let mut end = curr;
                        if shift_down {
                            let dx = curr.x - start.x;
                            let dy = curr.y - start.y;
                            let side = dx.abs().max(dy.abs());
                            let side_x = if dx >= 0.0 { side } else { -side };
                            let side_y = if dy >= 0.0 { side } else { -side };
                            end = egui::pos2(start.x + side_x, start.y + side_y);
                        }

                        let min_x = start.x.min(end.x).max(0.0);
                        let min_y = start.y.min(end.y).max(0.0);
                        let max_x = start.x.max(end.x).min(app.document.width as f32);
                        let max_y = start.y.max(end.y).min(app.document.height as f32);
                        let w = (max_x - min_x).max(0.0);
                        let h = (max_y - min_y).max(0.0);

                        if w >= 1.0 && h >= 1.0 {
                            let ux = min_x.floor() as u32;
                            let uy = min_y.floor() as u32;
                            let uw = w.ceil() as u32;
                            let uh = h.ceil() as u32;

                            match app.active_tool {
                                ActiveTool::SelectRect => {
                                    app.selection.select_rect(ux, uy, uw, uh);
                                    app.selection_shape = SelectionShape::Rect;
                                }
                                ActiveTool::SelectEllipse => {
                                    app.selection.select_ellipse(ux, uy, uw, uh);
                                    app.selection_shape = SelectionShape::Ellipse;
                                }
                                _ => {}
                            }
                        } else {
                            app.selection.clear();
                        }
                    }
                    app.selection_drag_start = None;
                    app.selection_drag_current = None;
                    ctx.request_repaint();
                }
            } else if app.selection_drag_start.is_some() {
                app.selection_drag_start = None;
                app.selection_drag_current = None;
            }

            // Obsługa narzędzia Przesuwanie (M): przeciąganie przesuwa piksele aktywnej warstwy lub zaznaczenia
            let can_move = app.filter_dialog.is_none()
                && app.active_tool == ActiveTool::Move
                && !space_down
                && !is_middle_down;

            if can_move {
                if is_primary_down && (is_hovered || app.is_moving) {
                    if let Some(pos) = hover_pos {
                        let canvas_pos = app.transform.screen_to_canvas(pos);
                        if !app.is_moving {
                            app.start_move(canvas_pos);
                        } else if let Some(start) = app.move_drag_start {
                            let dx = (canvas_pos.x - start.x).round() as i32;
                            let dy = (canvas_pos.y - start.y).round() as i32;
                            app.apply_move_offset(dx, dy);
                        }
                        ctx.request_repaint();
                    }
                } else if !is_primary_down && app.is_moving {
                    app.finish_move();
                    ctx.request_repaint();
                }
            } else if app.is_moving {
                app.finish_move();
                ctx.request_repaint();
            }

            // Obsługa narzędzia Kadrowanie (Shift+C): przeciąganie definiuje ramkę kadrowania
            let can_crop = app.filter_dialog.is_none()
                && app.active_tool == ActiveTool::Crop
                && !space_down
                && !is_middle_down;

            if can_crop {
                if is_primary_down && (is_hovered || app.crop_drag_start.is_some()) {
                    if let Some(pos) = hover_pos {
                        let canvas_pos = app.transform.screen_to_canvas(pos);
                        if app.crop_drag_start.is_none() {
                            app.crop_drag_start = Some(canvas_pos);
                        }
                        app.crop_drag_current = Some(canvas_pos);
                        ctx.request_repaint();
                    }
                } else if !is_primary_down && app.crop_drag_start.is_some() {
                    if let (Some(start), Some(curr)) = (app.crop_drag_start, app.crop_drag_current)
                    {
                        let min_x = start.x.min(curr.x).max(0.0);
                        let min_y = start.y.min(curr.y).max(0.0);
                        let max_x = start.x.max(curr.x).min(app.document.width as f32);
                        let max_y = start.y.max(curr.y).min(app.document.height as f32);
                        let w = (max_x - min_x).max(0.0);
                        let h = (max_y - min_y).max(0.0);

                        if w >= 1.0 && h >= 1.0 {
                            app.crop_rect = Some(dziwak_core::Rect::new(
                                min_x.floor() as u32,
                                min_y.floor() as u32,
                                w.ceil() as u32,
                                h.ceil() as u32,
                            ));
                        }
                    }
                    app.crop_drag_start = None;
                    app.crop_drag_current = None;
                    ctx.request_repaint();
                }
            }

            // Obsługa narzędzia Zaznaczenie odręczne (F): klikane wielokąty / przeciąganie, dwuklik lub Enter zamyka
            let can_free_select = app.filter_dialog.is_none()
                && app.active_tool == ActiveTool::SelectFree
                && !space_down
                && !is_middle_down;

            if can_free_select {
                let double_clicked = ctx.input(|i| {
                    i.pointer
                        .button_double_clicked(egui::PointerButton::Primary)
                });
                if double_clicked {
                    app.finish_free_select();
                    ctx.request_repaint();
                } else if is_hovered {
                    if let Some(pos) = hover_pos {
                        let canvas_pos = app.transform.screen_to_canvas(pos);
                        let pt = (canvas_pos.x, canvas_pos.y);

                        let mods = ctx.input(|i| i.modifiers);
                        let mode = match (mods.shift, mods.command) {
                            (true, true) => dziwak_core::selection::SelectMode::Intersect,
                            (true, false) => dziwak_core::selection::SelectMode::Add,
                            (false, true) => dziwak_core::selection::SelectMode::Subtract,
                            (false, false) => dziwak_core::selection::SelectMode::Replace,
                        };

                        if is_primary_pressed {
                            let close_to_start =
                                if let Some(&start) = app.free_select_points.first() {
                                    if app.free_select_points.len() >= 3 {
                                        let dx = pt.0 - start.0;
                                        let dy = pt.1 - start.1;
                                        (dx * dx + dy * dy).sqrt() < 8.0
                                    } else {
                                        false
                                    }
                                } else {
                                    false
                                };

                            if close_to_start {
                                app.finish_free_select();
                            } else {
                                app.add_free_select_point(pt, mode);
                            }
                            ctx.request_repaint();
                        } else if is_primary_down && !app.free_select_points.is_empty() {
                            if let Some(&last) = app.free_select_points.last() {
                                let dx = pt.0 - last.0;
                                let dy = pt.1 - last.1;
                                if (dx * dx + dy * dy).sqrt() >= 3.0 {
                                    app.add_free_select_point(pt, mode);
                                    ctx.request_repaint();
                                }
                            }
                        }
                    }
                }
            } else if app.active_tool != ActiveTool::SelectFree
                && !app.free_select_points.is_empty()
            {
                app.cancel_free_select();
            }

            // Animacja maszerujących mrówek (tylko gdy aktywne jest zaznaczenie lub przeciąganie)
            let has_active_selection =
                app.selection.has_selection && app.selection.bounds().is_some();
            let is_dragging_selection = app.selection_drag_start.is_some();
            let has_crop_overlay = app.active_tool == ActiveTool::Crop
                && (app.crop_rect.is_some() || app.crop_drag_start.is_some());
            let has_free_select =
                app.active_tool == ActiveTool::SelectFree && !app.free_select_points.is_empty();
            let has_zoom_drag =
                app.active_tool == ActiveTool::Zoom && app.zoom_drag_start.is_some();
            if has_active_selection
                || is_dragging_selection
                || has_crop_overlay
                || has_free_select
                || has_zoom_drag
            {
                ctx.request_repaint_after(std::time::Duration::from_millis(150));
            }

            // Kursor myszy w zależności od trybu nawigacji i narzędzia
            if app.is_panning {
                ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
            } else if (space_down || app.active_tool == ActiveTool::Pan) && is_hovered {
                ctx.set_cursor_icon(egui::CursorIcon::Grab);
            } else if app.active_tool == ActiveTool::Move && is_hovered {
                if app.is_moving {
                    ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
                } else {
                    ctx.set_cursor_icon(egui::CursorIcon::Move);
                }
            } else if app.active_tool == ActiveTool::Zoom && is_hovered {
                let zoom_out = alt_down || ctx.input(|i| i.modifiers.command || i.modifiers.ctrl);
                if zoom_out {
                    ctx.set_cursor_icon(egui::CursorIcon::ZoomOut);
                } else {
                    ctx.set_cursor_icon(egui::CursorIcon::ZoomIn);
                }
            } else if is_hovered {
                ctx.set_cursor_icon(egui::CursorIcon::Crosshair);
            }

            // Obsługa zoomu kółkiem myszy wokół kursora
            if is_hovered {
                let scroll_y = ctx.input(|i| {
                    if i.smooth_scroll_delta.y != 0.0 {
                        i.smooth_scroll_delta.y
                    } else {
                        i.raw_scroll_delta.y * 50.0
                    }
                });
                let pinch_zoom = ctx.input(|i| i.zoom_delta());

                let zoom_factor = pinch_zoom * 1.15_f32.powf(scroll_y / 50.0);
                if (zoom_factor - 1.0_f32).abs() > 1e-4 {
                    if let Some(cursor_pos) = hover_pos {
                        app.transform
                            .zoom_around(cursor_pos, zoom_factor, 0.05, 64.0);
                        app.auto_fit = false;
                    }
                }
            }

            // Rysowanie płótna i linijek
            let painter = ui.painter_at(response_rect);

            // Wypełnienie tła poza obrazem (#262626)
            let canvas_painter = painter.with_clip_rect(canvas_viewport_rect);
            canvas_painter.rect_filled(
                canvas_viewport_rect,
                0.0_f32,
                egui::Color32::from_rgb(0x26, 0x26, 0x26),
            );

            let canvas_size = egui::vec2(app.document.width as f32, app.document.height as f32);
            let canvas_screen_rect = egui::Rect::from_min_size(
                egui::pos2(app.transform.pan.x, app.transform.pan.y),
                canvas_size * app.transform.zoom,
            );

            // Cień pod płótnem
            let shadow_rect = canvas_screen_rect
                .expand(4.0_f32)
                .translate(egui::vec2(2.0_f32, 4.0_f32));
            canvas_painter.rect_filled(
                shadow_rect,
                2.0_f32,
                egui::Color32::from_rgba_unmultiplied(0, 0, 0, 90),
            );

            // Szachownica 8x8 pod przezroczystością (ładowana raz na GPU z TextureWrapMode::Repeat)
            let checker_tex = app.checkerboard_texture.get_or_insert_with(|| {
                let mut pixels = Vec::with_capacity(16 * 16);
                for y in 0..16 {
                    for x in 0..16 {
                        let is_light = ((x / 8) + (y / 8)) % 2 == 0;
                        let c = if is_light {
                            egui::Color32::from_gray(240)
                        } else {
                            egui::Color32::from_gray(200)
                        };
                        pixels.push(c);
                    }
                }
                let image = egui::ColorImage {
                    size: [16, 16],
                    source_size: egui::vec2(16.0_f32, 16.0_f32),
                    pixels,
                };
                ctx.load_texture(
                    "checkerboard_8x8",
                    image,
                    egui::TextureOptions {
                        magnification: egui::TextureFilter::Nearest,
                        minification: egui::TextureFilter::Nearest,
                        wrap_mode: egui::TextureWrapMode::Repeat,
                        mipmap_mode: None,
                    },
                )
            });

            let uv_min = egui::pos2(
                canvas_screen_rect.left() / 16.0_f32,
                canvas_screen_rect.top() / 16.0_f32,
            );
            let uv_max = egui::pos2(
                canvas_screen_rect.right() / 16.0_f32,
                canvas_screen_rect.bottom() / 16.0_f32,
            );
            let uv_rect = egui::Rect::from_min_max(uv_min, uv_max);

            canvas_painter.image(
                checker_tex.id(),
                canvas_screen_rect,
                uv_rect,
                egui::Color32::WHITE,
            );

            // Aktualizacja tekstur GPU dla brudnych kafli (tylko zmienione kafle są komponowane)
            app.tile_renderer.set_zoom(app.transform.zoom);
            app.tile_renderer.update_dirty_tiles(ctx, &app.document);

            // Rysujemy skomponowane kafle dokumentu przesłane na GPU
            app.tile_renderer.draw_visible_tiles(
                &canvas_painter,
                &app.transform,
                app.document.width,
                app.document.height,
                canvas_viewport_rect,
                app.show_tile_grid,
            );

            // Rysowanie maszerujących mrówek (obrysu zaznaczenia)
            let time = ctx.input(|i| i.time);
            let phase = ((time / 0.150).floor() as i64).rem_euclid(8) as f32;

            if let (Some(start), Some(curr)) =
                (app.selection_drag_start, app.selection_drag_current)
            {
                let shift_down = ctx.input(|i| i.modifiers.shift);
                let mut end = curr;
                if shift_down {
                    let dx = curr.x - start.x;
                    let dy = curr.y - start.y;
                    let side = dx.abs().max(dy.abs());
                    let side_x = if dx >= 0.0 { side } else { -side };
                    let side_y = if dy >= 0.0 { side } else { -side };
                    end = egui::pos2(start.x + side_x, start.y + side_y);
                }

                let min_x = start.x.min(end.x).max(0.0);
                let min_y = start.y.min(end.y).max(0.0);
                let max_x = start.x.max(end.x).min(app.document.width as f32);
                let max_y = start.y.max(end.y).min(app.document.height as f32);

                let s_min = app.transform.canvas_to_screen(egui::pos2(min_x, min_y));
                let s_max = app.transform.canvas_to_screen(egui::pos2(max_x, max_y));
                let drag_screen_rect = egui::Rect::from_min_max(s_min, s_max);

                match app.active_tool {
                    ActiveTool::SelectEllipse => {
                        draw_marching_ants_ellipse(&canvas_painter, drag_screen_rect, phase);
                    }
                    _ => {
                        draw_marching_ants_rect(&canvas_painter, drag_screen_rect, phase);
                    }
                }
            } else if app.selection.has_selection {
                if let Some(b) = app.selection.bounds() {
                    let s_min = app
                        .transform
                        .canvas_to_screen(egui::pos2(b.x as f32, b.y as f32));
                    let s_max = app.transform.canvas_to_screen(egui::pos2(
                        (b.x + b.width) as f32,
                        (b.y + b.height) as f32,
                    ));
                    let sel_screen_rect = egui::Rect::from_min_max(s_min, s_max);

                    match app.selection_shape {
                        SelectionShape::Ellipse => {
                            draw_marching_ants_ellipse(&canvas_painter, sel_screen_rect, phase);
                        }
                        SelectionShape::Rect => {
                            draw_marching_ants_rect(&canvas_painter, sel_screen_rect, phase);
                        }
                    }
                }
            }

            // Podgląd linii gradientu podczas przeciągania
            if let (Some(start), Some(curr)) = (app.gradient_drag_start, app.gradient_drag_current)
            {
                let s_start = app.transform.canvas_to_screen(start);
                let s_curr = app.transform.canvas_to_screen(curr);

                canvas_painter.line_segment(
                    [s_start, s_curr],
                    egui::Stroke::new(3.0_f32, egui::Color32::from_black_alpha(160)),
                );
                canvas_painter.line_segment(
                    [s_start, s_curr],
                    egui::Stroke::new(1.5_f32, egui::Color32::WHITE),
                );

                canvas_painter.circle_filled(s_start, 5.0_f32, app.brush_color);
                canvas_painter.circle_stroke(
                    s_start,
                    5.0_f32,
                    egui::Stroke::new(1.5_f32, egui::Color32::WHITE),
                );

                canvas_painter.circle_filled(s_curr, 5.0_f32, app.bg_color);
                canvas_painter.circle_stroke(
                    s_curr,
                    5.0_f32,
                    egui::Stroke::new(1.5_f32, egui::Color32::WHITE),
                );
            }

            // Podgląd obrysu pędzla / ołówka / aerografu / gumki pod kursorem
            if is_hovered
                && !alt_down
                && matches!(
                    app.active_tool,
                    ActiveTool::Brush
                        | ActiveTool::Pencil
                        | ActiveTool::Airbrush
                        | ActiveTool::Eraser
                        | ActiveTool::Clone
                        | ActiveTool::Smudge
                        | ActiveTool::DodgeBurn
                        | ActiveTool::BlurSharpen
                )
            {
                if let Some(pos) = hover_pos {
                    let r = (app.brush_size * 0.5 * app.transform.zoom).max(1.0);
                    canvas_painter.circle_stroke(
                        pos,
                        r,
                        egui::Stroke::new(
                            1.0_f32,
                            egui::Color32::from_rgba_unmultiplied(220, 220, 220, 180),
                        ),
                    );
                }
            }

            // Rysowanie ramki kadrowania (T2)
            if app.active_tool == ActiveTool::Crop {
                let active_crop_rect = if let (Some(start), Some(curr)) =
                    (app.crop_drag_start, app.crop_drag_current)
                {
                    let min_x = start.x.min(curr.x).max(0.0);
                    let min_y = start.y.min(curr.y).max(0.0);
                    let max_x = start.x.max(curr.x).min(app.document.width as f32);
                    let max_y = start.y.max(curr.y).min(app.document.height as f32);
                    let w = (max_x - min_x).max(0.0);
                    let h = (max_y - min_y).max(0.0);
                    if w >= 1.0 && h >= 1.0 {
                        Some(dziwak_core::Rect::new(
                            min_x.floor() as u32,
                            min_y.floor() as u32,
                            w.ceil() as u32,
                            h.ceil() as u32,
                        ))
                    } else {
                        None
                    }
                } else {
                    app.crop_rect
                };

                if let Some(cr) = active_crop_rect {
                    let s_min = app
                        .transform
                        .canvas_to_screen(egui::pos2(cr.x as f32, cr.y as f32));
                    let s_max = app.transform.canvas_to_screen(egui::pos2(
                        (cr.x + cr.width) as f32,
                        (cr.y + cr.height) as f32,
                    ));
                    let crop_screen_rect = egui::Rect::from_min_max(s_min, s_max);

                    let dim_color = egui::Color32::from_black_alpha(140);
                    // Góra
                    if crop_screen_rect.top() > canvas_screen_rect.top() {
                        let r = egui::Rect::from_min_max(
                            canvas_screen_rect.min,
                            egui::pos2(canvas_screen_rect.max.x, crop_screen_rect.top()),
                        );
                        canvas_painter.rect_filled(r, 0.0, dim_color);
                    }
                    // Dół
                    if crop_screen_rect.bottom() < canvas_screen_rect.bottom() {
                        let r = egui::Rect::from_min_max(
                            egui::pos2(canvas_screen_rect.min.x, crop_screen_rect.bottom()),
                            canvas_screen_rect.max,
                        );
                        canvas_painter.rect_filled(r, 0.0, dim_color);
                    }
                    // Lewa strona
                    let mid_top = crop_screen_rect.top().max(canvas_screen_rect.top());
                    let mid_bottom = crop_screen_rect.bottom().min(canvas_screen_rect.bottom());
                    if crop_screen_rect.left() > canvas_screen_rect.left() && mid_bottom > mid_top {
                        let r = egui::Rect::from_min_max(
                            egui::pos2(canvas_screen_rect.min.x, mid_top),
                            egui::pos2(crop_screen_rect.left(), mid_bottom),
                        );
                        canvas_painter.rect_filled(r, 0.0, dim_color);
                    }
                    // Prawa strona
                    if crop_screen_rect.right() < canvas_screen_rect.right() && mid_bottom > mid_top
                    {
                        let r = egui::Rect::from_min_max(
                            egui::pos2(crop_screen_rect.right(), mid_top),
                            egui::pos2(canvas_screen_rect.max.x, mid_bottom),
                        );
                        canvas_painter.rect_filled(r, 0.0, dim_color);
                    }

                    // Obrys ramki kadrowania
                    draw_marching_ants_rect(&canvas_painter, crop_screen_rect, phase);
                }
            }

            // Rysowanie ścieżki zaznaczenia odręcznego (lasso, T3)
            if app.active_tool == ActiveTool::SelectFree && !app.free_select_points.is_empty() {
                let n = app.free_select_points.len();
                let screen_pts: Vec<egui::Pos2> = app
                    .free_select_points
                    .iter()
                    .map(|&(x, y)| app.transform.canvas_to_screen(egui::pos2(x, y)))
                    .collect();

                // Rysujemy linie łączące punkty
                for i in 0..n - 1 {
                    let p0 = screen_pts[i];
                    let p1 = screen_pts[i + 1];
                    canvas_painter.line_segment(
                        [p0, p1],
                        egui::Stroke::new(3.0_f32, egui::Color32::from_black_alpha(180)),
                    );
                    canvas_painter
                        .line_segment([p0, p1], egui::Stroke::new(1.5_f32, egui::Color32::WHITE));
                }

                // Linia podglądu od ostatniego punktu do bieżącej pozycji kursora
                if is_hovered {
                    if let Some(pos) = hover_pos {
                        let last_screen = screen_pts[n - 1];
                        canvas_painter.line_segment(
                            [last_screen, pos],
                            egui::Stroke::new(2.0_f32, egui::Color32::from_black_alpha(140)),
                        );
                        canvas_painter.line_segment(
                            [last_screen, pos],
                            egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(255, 255, 100)),
                        );
                    }
                }

                // Zaznacz pierwszy punkt kółkiem (ułatwienie zamknięcia wielokąta)
                let first_screen = screen_pts[0];
                canvas_painter.circle_filled(
                    first_screen,
                    5.0_f32,
                    egui::Color32::from_rgb(255, 200, 0),
                );
                canvas_painter.circle_stroke(
                    first_screen,
                    5.0_f32,
                    egui::Stroke::new(1.5_f32, egui::Color32::BLACK),
                );
            }

            // Rysowanie wskaźnika źródła klonowania (T8)
            if app.active_tool == ActiveTool::Clone {
                let source_coord = if let Some((dx, dy)) = app.clone_offset {
                    if let Some(pos) = hover_pos {
                        let cpos = app.transform.screen_to_canvas(pos);
                        Some((cpos.x + dx as f32, cpos.y + dy as f32))
                    } else {
                        app.clone_source
                    }
                } else {
                    app.clone_source
                };

                if let Some((sx, sy)) = source_coord {
                    let sp = app.transform.canvas_to_screen(egui::pos2(sx, sy));
                    let r = 8.0_f32;
                    canvas_painter.circle_stroke(
                        sp,
                        r,
                        egui::Stroke::new(2.0_f32, egui::Color32::from_black_alpha(180)),
                    );
                    canvas_painter.circle_stroke(
                        sp,
                        r,
                        egui::Stroke::new(1.0_f32, egui::Color32::WHITE),
                    );
                    canvas_painter.line_segment(
                        [
                            egui::pos2(sp.x - r - 3.0, sp.y),
                            egui::pos2(sp.x + r + 3.0, sp.y),
                        ],
                        egui::Stroke::new(1.0_f32, egui::Color32::WHITE),
                    );
                    canvas_painter.line_segment(
                        [
                            egui::pos2(sp.x, sp.y - r - 3.0),
                            egui::pos2(sp.x, sp.y + r + 3.0),
                        ],
                        egui::Stroke::new(1.0_f32, egui::Color32::WHITE),
                    );
                }
            }

            // Podgląd tekstu na płótnie (T10)
            if let Some(dialog) = &app.text_dialog {
                if let Some(mask) = &dialog.preview_mask {
                    let s_min = app
                        .transform
                        .canvas_to_screen(egui::pos2(dialog.pos.0 as f32, dialog.pos.1 as f32));
                    let s_max = app.transform.canvas_to_screen(egui::pos2(
                        (dialog.pos.0 + mask.width as i32) as f32,
                        (dialog.pos.1 + mask.height as i32) as f32,
                    ));
                    let text_screen_rect = egui::Rect::from_min_max(s_min, s_max);

                    // Ramka przerywana
                    draw_marching_ants_rect(&canvas_painter, text_screen_rect, phase);

                    // Rysowanie tekstu
                    let scaled_font_size = (dialog.font_size * app.transform.zoom).max(6.0_f32);
                    canvas_painter.text(
                        s_min,
                        egui::Align2::LEFT_TOP,
                        &dialog.text,
                        egui::FontId::proportional(scaled_font_size),
                        dialog.color,
                    );
                }
            }

            // Cienka ramka wokół obrazu
            canvas_painter.rect_stroke(
                canvas_screen_rect,
                0.0_f32,
                egui::Stroke::new(1.0_f32, egui::Color32::BLACK),
                egui::StrokeKind::Outside,
            );

            // --- RYSOWANIE LINIJEK (G2) ---
            let ruler_bg = egui::Color32::from_rgb(42, 42, 42);
            let ruler_border = egui::Color32::from_rgb(56, 56, 56);

            // Narożnik linijek (18x18 px)
            painter.rect_filled(corner_rect, 0.0_f32, ruler_bg);
            painter.rect_stroke(
                corner_rect,
                0.0_f32,
                egui::Stroke::new(1.0_f32, ruler_border),
                egui::StrokeKind::Inside,
            );
            painter.text(
                corner_rect.center(),
                egui::Align2::CENTER_CENTER,
                "px",
                egui::FontId::proportional(8.5),
                egui::Color32::from_gray(140),
            );

            // Górna linijka (18 px)
            painter.rect_filled(top_ruler_rect, 0.0_f32, ruler_bg);
            painter.line_segment(
                [
                    egui::pos2(top_ruler_rect.left(), top_ruler_rect.bottom()),
                    egui::pos2(top_ruler_rect.right(), top_ruler_rect.bottom()),
                ],
                egui::Stroke::new(1.0_f32, ruler_border),
            );

            let offset_x = app.transform.pan.x - top_ruler_rect.left();
            ruler_ticks(
                offset_x,
                app.transform.zoom,
                top_ruler_rect.width(),
                &mut app.ruler_ticks_h,
            );

            let top_ruler_painter = painter.with_clip_rect(top_ruler_rect);
            for tick in &app.ruler_ticks_h {
                let x = top_ruler_rect.left() + tick.pos;
                if x < top_ruler_rect.left() || x > top_ruler_rect.right() {
                    continue;
                }
                if tick.major {
                    top_ruler_painter.line_segment(
                        [
                            egui::pos2(x, top_ruler_rect.bottom() - 7.0_f32),
                            egui::pos2(x, top_ruler_rect.bottom()),
                        ],
                        egui::Stroke::new(1.0_f32, egui::Color32::from_gray(175)),
                    );
                    if let Some(val) = tick.label {
                        top_ruler_painter.text(
                            egui::pos2(x + 2.0_f32, top_ruler_rect.top() + 1.0_f32),
                            egui::Align2::LEFT_TOP,
                            format!("{val}"),
                            egui::FontId::proportional(8.5),
                            egui::Color32::from_gray(190),
                        );
                    }
                } else {
                    top_ruler_painter.line_segment(
                        [
                            egui::pos2(x, top_ruler_rect.bottom() - 4.0_f32),
                            egui::pos2(x, top_ruler_rect.bottom()),
                        ],
                        egui::Stroke::new(1.0_f32, egui::Color32::from_gray(110)),
                    );
                }
            }

            // Lewa linijka (18 px)
            painter.rect_filled(left_ruler_rect, 0.0_f32, ruler_bg);
            painter.line_segment(
                [
                    egui::pos2(left_ruler_rect.right(), left_ruler_rect.top()),
                    egui::pos2(left_ruler_rect.right(), left_ruler_rect.bottom()),
                ],
                egui::Stroke::new(1.0_f32, ruler_border),
            );

            let offset_y = app.transform.pan.y - left_ruler_rect.top();
            ruler_ticks(
                offset_y,
                app.transform.zoom,
                left_ruler_rect.height(),
                &mut app.ruler_ticks_v,
            );

            let left_ruler_painter = painter.with_clip_rect(left_ruler_rect);
            for tick in &app.ruler_ticks_v {
                let y = left_ruler_rect.top() + tick.pos;
                if y < left_ruler_rect.top() || y > left_ruler_rect.bottom() {
                    continue;
                }
                if tick.major {
                    left_ruler_painter.line_segment(
                        [
                            egui::pos2(left_ruler_rect.right() - 7.0_f32, y),
                            egui::pos2(left_ruler_rect.right(), y),
                        ],
                        egui::Stroke::new(1.0_f32, egui::Color32::from_gray(175)),
                    );
                    if let Some(val) = tick.label {
                        left_ruler_painter.text(
                            egui::pos2(left_ruler_rect.left() + 1.0_f32, y + 1.0_f32),
                            egui::Align2::LEFT_TOP,
                            format!("{val}"),
                            egui::FontId::proportional(7.5),
                            egui::Color32::from_gray(190),
                        );
                    }
                } else {
                    left_ruler_painter.line_segment(
                        [
                            egui::pos2(left_ruler_rect.right() - 4.0_f32, y),
                            egui::pos2(left_ruler_rect.right(), y),
                        ],
                        egui::Stroke::new(1.0_f32, egui::Color32::from_gray(110)),
                    );
                }
            }

            // Znacznik pozycji kursora na linijkach (czerwona linia prowadząca)
            if let Some(pos) = hover_pos {
                let marker_color = egui::Color32::from_rgb(225, 60, 60);
                if pos.x >= top_ruler_rect.left() && pos.x <= top_ruler_rect.right() {
                    painter.line_segment(
                        [
                            egui::pos2(pos.x, top_ruler_rect.top()),
                            egui::pos2(pos.x, top_ruler_rect.bottom()),
                        ],
                        egui::Stroke::new(1.0_f32, marker_color),
                    );
                }
                if pos.y >= left_ruler_rect.top() && pos.y <= left_ruler_rect.bottom() {
                    painter.line_segment(
                        [
                            egui::pos2(left_ruler_rect.left(), pos.y),
                            egui::pos2(left_ruler_rect.right(), pos.y),
                        ],
                        egui::Stroke::new(1.0_f32, marker_color),
                    );
                }
            }
        });
}
