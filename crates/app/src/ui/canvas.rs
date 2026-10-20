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
                (canvas_viewport_rect.size() - app.doc.last_viewport_size).length() > 0.5;
            if viewport_valid {
                if should_fit_to_viewport {
                    app.doc.auto_fit = true;
                }
                if !app.doc.initialized
                    || should_fit_to_viewport
                    || (app.doc.auto_fit && size_changed)
                {
                    app.fit_to_viewport(canvas_viewport_rect);
                    app.doc.initialized = true;
                } else if app.doc.center_requested {
                    app.center_canvas(canvas_viewport_rect);
                }
                app.doc.center_requested = false;
                app.doc.last_viewport_size = canvas_viewport_rect.size();
            }

            // Obszar interaktywny dla całego widoku
            let (response_rect, _canvas_response) =
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

            // Płótno reaguje tylko na mysz nad nim, gdy nic go nie zasłania (okna filtrów, menu),
            // a przeciąganie tylko wtedy, gdy przycisk wciśnięto na płótnie — przeciągnięcie suwaka
            // w oknie nie może przesuwać obrazu ani malować.
            // Najwyższa warstwa egui pod kursorem musi być warstwą płótna — okna (filtry, dialogi)
            // i menu leżą na wyższych warstwach i zasłaniają je.
            let canvas_layer = ui.layer_id();
            let over_canvas = hover_pos.is_some_and(|pos| {
                canvas_viewport_rect.contains(pos)
                    && ctx.layer_id_at(pos).is_none_or(|l| l == canvas_layer)
            });
            let (any_pressed, any_down) = ctx.input(|i| {
                (
                    i.pointer.any_pressed(),
                    i.pointer.button_down(egui::PointerButton::Primary)
                        || i.pointer.button_down(egui::PointerButton::Middle),
                )
            });
            if any_pressed {
                app.canvas_owns_pointer = over_canvas;
            } else if !any_down {
                app.canvas_owns_pointer = false;
            }
            let is_primary_down = is_primary_down && app.canvas_owns_pointer;
            let is_middle_down = is_middle_down && app.canvas_owns_pointer;
            let is_primary_pressed = is_primary_pressed && over_canvas;
            let is_hovered = over_canvas;
            app.cursor_canvas_pos = if is_hovered {
                hover_pos.map(|pos| app.doc.transform.screen_to_canvas(pos))
            } else {
                None
            };

            // Obsługa przesuwania widoku (ŚPM lub Spacja + LPM, lub narzędzie Przesuwanie + LPM)
            let pan_active = is_middle_down
                || (space_down && is_primary_down)
                || (app.active_tool == ActiveTool::Pan && is_primary_down);

            if pan_active && (is_hovered || app.is_panning) {
                app.is_panning = true;
                app.doc.transform.pan += pointer_delta;
                app.doc.auto_fit = false;
            } else if !pan_active {
                app.is_panning = false;
            }

            // Obsługa pipety (narzędzie Pipeta lub Alt+klik w Pędzlu/Ołówku/Aerografie)
            let can_sample = app.doc.filter_dialog.is_none()
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
                        let canvas_pos = app.doc.transform.screen_to_canvas(pos);
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
            let can_paint = app.doc.filter_dialog.is_none()
                && is_brush_like
                && !space_down
                && !is_middle_down
                && !alt_down;

            if can_paint {
                if is_primary_down && (is_hovered || app.doc.is_painting) {
                    if let Some(pos) = hover_pos {
                        let canvas_pos = app.doc.transform.screen_to_canvas(pos);
                        let p2 = (canvas_pos.x, canvas_pos.y);
                        let spacing = (app.brush_size * 0.15).max(1.0);

                        if !app.doc.is_painting {
                            // Początek pociągnięcia: zapisujemy stan w historii raz na pociągnięcie
                            app.push_history();
                            app.doc.is_painting = true;

                            // Na początku pociągnięcia carry = spacing, aby pierwszy punkt był w p2
                            app.doc.stroke_carry = spacing;
                            app.apply_tool_dab(p2.0, p2.1);
                            app.doc.last_stroke_pos = Some(p2);
                            if app.active_tool == ActiveTool::Airbrush {
                                app.doc.last_airbrush_dab = Some(Instant::now());
                                ctx.request_repaint_after(Duration::from_millis(50));
                            }
                        } else {
                            if let Some(p1) = app.doc.last_stroke_pos {
                                let mut points = Vec::new();
                                app.doc.stroke_carry = interpolate_stroke(
                                    p1,
                                    p2,
                                    spacing,
                                    app.doc.stroke_carry,
                                    &mut points,
                                );
                                for pt in points {
                                    app.apply_tool_dab(pt.0, pt.1);
                                }
                                app.doc.last_stroke_pos = Some(p2);
                            }

                            if app.active_tool == ActiveTool::Airbrush {
                                let now = Instant::now();
                                let should_dab = match app.doc.last_airbrush_dab {
                                    None => true,
                                    Some(t) => now.duration_since(t) >= Duration::from_millis(50),
                                };
                                if should_dab {
                                    app.apply_tool_dab(p2.0, p2.1);
                                    app.doc.last_airbrush_dab = Some(now);
                                }
                                let elapsed = app
                                    .doc
                                    .last_airbrush_dab
                                    .map(|t| now.duration_since(t))
                                    .unwrap_or_default();
                                let remaining = Duration::from_millis(50).saturating_sub(elapsed);
                                ctx.request_repaint_after(remaining);
                            }
                        }
                        ctx.request_repaint();
                    }
                } else if !is_primary_down && app.doc.is_painting {
                    app.doc.is_painting = false;
                    app.doc.last_stroke_pos = None;
                    app.doc.last_airbrush_dab = None;
                }
            } else if app.doc.is_painting {
                app.doc.is_painting = false;
                app.doc.last_stroke_pos = None;
                app.doc.last_airbrush_dab = None;
            }

            // Obsługa klonowania (C): Ctrl+klik = źródło, malowanie LPM kopiuje z przesunięciem (T8)
            let can_clone = app.doc.filter_dialog.is_none()
                && app.active_tool == ActiveTool::Clone
                && !space_down
                && !is_middle_down
                && !alt_down;

            if can_clone {
                let is_ctrl_down = ctx.input(|i| i.modifiers.command);
                if is_ctrl_down && is_primary_pressed && is_hovered {
                    if let Some(pos) = hover_pos {
                        let canvas_pos = app.doc.transform.screen_to_canvas(pos);
                        app.set_clone_source((canvas_pos.x, canvas_pos.y));
                        ctx.request_repaint();
                    }
                } else if !is_ctrl_down && app.doc.clone_source.is_some() {
                    if is_primary_down && (is_hovered || app.doc.is_painting) {
                        if let Some(pos) = hover_pos {
                            let canvas_pos = app.doc.transform.screen_to_canvas(pos);
                            let p2 = (canvas_pos.x, canvas_pos.y);
                            let spacing = (app.brush_size * 0.15).max(1.0);

                            if !app.doc.is_painting {
                                app.doc.stroke_carry = spacing;
                                app.start_clone_stroke(p2);
                            } else if let Some(p1) = app.doc.last_stroke_pos {
                                let mut points = Vec::new();
                                app.doc.stroke_carry = interpolate_stroke(
                                    p1,
                                    p2,
                                    spacing,
                                    app.doc.stroke_carry,
                                    &mut points,
                                );
                                for pt in points {
                                    app.apply_clone_dab(pt.0, pt.1);
                                }
                                app.doc.last_stroke_pos = Some(p2);
                            }
                            ctx.request_repaint();
                        }
                    } else if !is_primary_down && app.doc.is_painting {
                        app.finish_clone_stroke();
                    }
                } else if !is_primary_down && app.doc.is_painting {
                    app.finish_clone_stroke();
                }
            } else if app.doc.is_painting && app.active_tool == ActiveTool::Clone {
                app.finish_clone_stroke();
            }

            // Obsługa rozmazywania (Shift+S, T9)
            let can_smudge = app.doc.filter_dialog.is_none()
                && app.active_tool == ActiveTool::Smudge
                && !space_down
                && !is_middle_down
                && !alt_down;

            if can_smudge {
                if is_primary_down && (is_hovered || app.doc.is_painting) {
                    if let Some(pos) = hover_pos {
                        let canvas_pos = app.doc.transform.screen_to_canvas(pos);
                        let p2 = (canvas_pos.x, canvas_pos.y);
                        let spacing = (app.brush_size * 0.15).max(1.0);

                        if !app.doc.is_painting {
                            app.push_history();
                            app.doc.is_painting = true;
                            app.doc.stroke_carry = spacing;
                            if !app.doc.document.layers.is_empty() {
                                let layer_idx = app
                                    .doc
                                    .active_layer_index
                                    .min(app.doc.document.layers.len() - 1);
                                let radius = (app.brush_size * 0.5).max(0.5);
                                app.doc.smudge_buffer = Some(dziwak_core::brush::smudge_begin(
                                    &app.doc.document.layers[layer_idx],
                                    p2.0,
                                    p2.1,
                                    radius,
                                ));
                            }
                            app.doc.last_stroke_pos = Some(p2);
                        } else if let Some(p1) = app.doc.last_stroke_pos {
                            let mut points = Vec::new();
                            app.doc.stroke_carry = interpolate_stroke(
                                p1,
                                p2,
                                spacing,
                                app.doc.stroke_carry,
                                &mut points,
                            );
                            for pt in points {
                                app.apply_smudge_dab(pt.0, pt.1);
                            }
                            app.doc.last_stroke_pos = Some(p2);
                        }
                        ctx.request_repaint();
                    }
                } else if !is_primary_down && app.doc.is_painting {
                    app.doc.is_painting = false;
                    app.doc.last_stroke_pos = None;
                    app.doc.smudge_buffer = None;
                }
            } else if app.doc.is_painting && app.active_tool == ActiveTool::Smudge {
                app.doc.is_painting = false;
                app.doc.last_stroke_pos = None;
                app.doc.smudge_buffer = None;
            }

            // Obsługa rozjaśniania/ściemniania (Shift+D) oraz rozmywania/wyostrzania (Shift+U)
            let is_dodge_or_blur = matches!(
                app.active_tool,
                ActiveTool::DodgeBurn | ActiveTool::BlurSharpen
            );
            let can_dodge_blur = app.doc.filter_dialog.is_none()
                && is_dodge_or_blur
                && !space_down
                && !is_middle_down
                && !alt_down;

            if can_dodge_blur {
                let invert_mode = ctx.input(|i| i.modifiers.command);
                if is_primary_down && (is_hovered || app.doc.is_painting) {
                    if let Some(pos) = hover_pos {
                        let canvas_pos = app.doc.transform.screen_to_canvas(pos);
                        let p2 = (canvas_pos.x, canvas_pos.y);
                        let spacing = (app.brush_size * 0.15).max(1.0);

                        let apply_dab = |app: &mut DziwakApp, x: f32, y: f32| {
                            if app.active_tool == ActiveTool::DodgeBurn {
                                app.apply_dodge_burn_dab(x, y, invert_mode);
                            } else if app.active_tool == ActiveTool::BlurSharpen {
                                app.apply_blur_sharpen_dab(x, y, invert_mode);
                            }
                        };

                        if !app.doc.is_painting {
                            app.push_history();
                            app.doc.is_painting = true;
                            app.doc.stroke_carry = spacing;
                            if app.active_tool == ActiveTool::BlurSharpen
                                && !app.doc.document.layers.is_empty()
                            {
                                let layer_idx = app
                                    .doc
                                    .active_layer_index
                                    .min(app.doc.document.layers.len() - 1);
                                app.doc.blur_sharpen_source =
                                    Some(app.doc.document.layers[layer_idx].pixels.clone());
                            }
                            apply_dab(app, p2.0, p2.1);
                            app.doc.last_stroke_pos = Some(p2);
                        } else if let Some(p1) = app.doc.last_stroke_pos {
                            let mut points = Vec::new();
                            app.doc.stroke_carry = interpolate_stroke(
                                p1,
                                p2,
                                spacing,
                                app.doc.stroke_carry,
                                &mut points,
                            );
                            for pt in points {
                                apply_dab(app, pt.0, pt.1);
                            }
                            app.doc.last_stroke_pos = Some(p2);
                        }
                        ctx.request_repaint();
                    }
                } else if !is_primary_down && app.doc.is_painting {
                    app.doc.is_painting = false;
                    app.doc.last_stroke_pos = None;
                    app.doc.blur_sharpen_source = None;
                }
            } else if app.doc.is_painting && is_dodge_or_blur {
                app.doc.is_painting = false;
                app.doc.last_stroke_pos = None;
                app.doc.blur_sharpen_source = None;
            }

            // Obsługa wypełniania kubełkiem (pojedyncze kliknięcie LPM na płótnie)
            let can_bucket = app.doc.filter_dialog.is_none()
                && app.active_tool == ActiveTool::Bucket
                && !space_down
                && !is_middle_down;

            // Różdżka: Shift = dodaj, Ctrl = odejmij, Shift+Ctrl = przetnij (jak w GIMP-ie)
            if app.doc.filter_dialog.is_none()
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
                    let canvas_pos = app.doc.transform.screen_to_canvas(pos);
                    app.apply_magic_wand(canvas_pos.x, canvas_pos.y, mode);
                    ctx.request_repaint();
                }
            }

            // Zaznaczenie wg koloru: Shift = dodaj, Ctrl = odejmij, Shift+Ctrl = przetnij (T4)
            if app.doc.filter_dialog.is_none()
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
                    let canvas_pos = app.doc.transform.screen_to_canvas(pos);
                    app.apply_select_by_color(canvas_pos.x, canvas_pos.y, mode);
                    ctx.request_repaint();
                }
            }

            if can_bucket && is_primary_pressed && is_hovered {
                if let Some(pos) = hover_pos {
                    let canvas_pos = app.doc.transform.screen_to_canvas(pos);
                    app.apply_bucket_fill(canvas_pos.x, canvas_pos.y);
                    ctx.request_repaint();
                }
            }

            // Narzędzie Tekst (T10): przeciągnięcie tworzy ramkę, krawędź przesuwa, klik poza ramką zatwierdza
            if app.active_tool != ActiveTool::Text && app.doc.text_editor.is_some() {
                app.commit_text();
            }

            let can_text = app.doc.filter_dialog.is_none()
                && app.active_tool == ActiveTool::Text
                && !space_down
                && !is_middle_down;

            if can_text {
                if app.doc.is_moving_text_box {
                    if is_primary_down {
                        if let Some(pos) = hover_pos {
                            let canvas_pos = app.doc.transform.screen_to_canvas(pos);
                            let new_x = (canvas_pos.x - app.doc.text_move_offset.x).round() as i32;
                            let new_y = (canvas_pos.y - app.doc.text_move_offset.y).round() as i32;
                            app.set_text_box_pos(new_x, new_y);
                            ctx.request_repaint();
                        }
                    } else {
                        app.doc.is_moving_text_box = false;
                        ctx.request_repaint();
                    }
                } else if app.doc.text_drag_start.is_some() {
                    if is_primary_down {
                        if let Some(pos) = hover_pos {
                            app.doc.text_drag_current =
                                Some(app.doc.transform.screen_to_canvas(pos));
                            ctx.request_repaint();
                        }
                    } else {
                        let start = app.doc.text_drag_start.take();
                        let curr = app.doc.text_drag_current.take().or(start);
                        if let (Some(p1), Some(p2)) = (start, curr) {
                            let s1 = app.doc.transform.canvas_to_screen(p1);
                            let s2 = app.doc.transform.canvas_to_screen(p2);
                            if (s2 - s1).length() < 4.0 {
                                app.start_text_box(
                                    p1.x.round() as i32,
                                    p1.y.round() as i32,
                                    0.0,
                                    0.0,
                                );
                            } else {
                                let min_x = p1.x.min(p2.x).round() as i32;
                                let min_y = p1.y.min(p2.y).round() as i32;
                                let w = (p2.x - p1.x).abs().round().max(1.0);
                                let h = (p2.y - p1.y).abs().round().max(1.0);
                                app.start_text_box(min_x, min_y, w, h);
                            }
                            ctx.request_repaint();
                        }
                    }
                } else if is_primary_pressed && is_hovered {
                    if let Some(pos) = hover_pos {
                        if let Some(focused_id) = ctx.memory(|m| m.focused()) {
                            ctx.memory_mut(|m| m.surrender_focus(focused_id));
                        }
                        let canvas_pos = app.doc.transform.screen_to_canvas(pos);
                        if let Some(box_canvas_rect) = app.text_box_canvas_rect() {
                            let s_min = app.doc.transform.canvas_to_screen(box_canvas_rect.min);
                            let s_max = app.doc.transform.canvas_to_screen(box_canvas_rect.max);
                            let screen_rect = egui::Rect::from_min_max(s_min, s_max);
                            let border = 6.0_f32;
                            let outer = screen_rect.expand(border);
                            let inner = screen_rect.shrink(border);
                            if outer.contains(pos) && (!inner.is_positive() || !inner.contains(pos))
                            {
                                if let Some(editor) = &app.doc.text_editor {
                                    app.doc.is_moving_text_box = true;
                                    app.doc.text_move_offset = canvas_pos
                                        - egui::pos2(editor.pos.0 as f32, editor.pos.1 as f32);
                                }
                            } else if inner.is_positive() && inner.contains(pos) {
                                app.set_text_cursor_from_canvas_pos(canvas_pos);
                            } else {
                                app.commit_text();
                            }
                        } else {
                            app.doc.text_drag_start = Some(canvas_pos);
                            app.doc.text_drag_current = Some(canvas_pos);
                        }
                        ctx.request_repaint();
                    }
                }
            } else {
                app.doc.is_moving_text_box = false;
                app.doc.text_drag_start = None;
                app.doc.text_drag_current = None;
            }

            // Obsługa narzędzia Lupa (Z): klik = powiększ, Alt/Ctrl+klik = pomniejsz, przeciągnięcie = powiększ obszar
            let can_zoom_tool = app.doc.filter_dialog.is_none()
                && app.active_tool == ActiveTool::Zoom
                && !space_down
                && !is_middle_down;

            if can_zoom_tool {
                let secondary_pressed =
                    ctx.input(|i| i.pointer.button_pressed(egui::PointerButton::Secondary));
                if secondary_pressed && is_hovered {
                    if let Some(pos) = hover_pos {
                        app.doc.transform.zoom_around(pos, 1.0 / 1.5, 0.05, 64.0);
                        app.doc.tile_renderer.set_zoom(app.doc.transform.zoom);
                        app.doc.auto_fit = false;
                        ctx.request_repaint();
                    }
                } else if is_primary_down && (is_hovered || app.doc.zoom_drag_start.is_some()) {
                    if let Some(pos) = hover_pos {
                        if app.doc.zoom_drag_start.is_none() {
                            app.doc.zoom_drag_start = Some(pos);
                        }
                        app.doc.zoom_drag_current = Some(pos);
                        ctx.request_repaint();
                    }
                } else if !is_primary_down && app.doc.zoom_drag_start.is_some() {
                    let start = app.doc.zoom_drag_start.take();
                    let curr = app.doc.zoom_drag_current.take().or(start);
                    if let (Some(p1), Some(p2)) = (start, curr) {
                        let drag_dist = (p2 - p1).length();
                        if drag_dist >= 5.0 {
                            let c1 = app.doc.transform.screen_to_canvas(p1);
                            let c2 = app.doc.transform.screen_to_canvas(p2);
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
                            app.doc.transform.zoom_around(p1, factor, 0.05, 64.0);
                            app.doc.tile_renderer.set_zoom(app.doc.transform.zoom);
                            app.doc.auto_fit = false;
                        }
                        ctx.request_repaint();
                    }
                }
            } else if app.doc.zoom_drag_start.is_some() {
                app.doc.zoom_drag_start = None;
                app.doc.zoom_drag_current = None;
            }

            // Obsługa narzędzia Miarka (Shift+M)
            let can_measure = app.doc.filter_dialog.is_none()
                && app.active_tool == ActiveTool::Measure
                && !space_down
                && !is_middle_down;

            if can_measure {
                if is_primary_down && (is_hovered || app.doc.is_measuring) {
                    if let Some(pos) = hover_pos {
                        let canvas_pos = app.doc.transform.screen_to_canvas(pos);
                        if !app.doc.is_measuring {
                            app.doc.measure_start = Some(canvas_pos);
                            app.doc.measure_end = Some(canvas_pos);
                            app.doc.is_measuring = true;
                        } else {
                            app.doc.measure_end = Some(canvas_pos);
                        }
                        ctx.request_repaint();
                    }
                } else if !is_primary_down && app.doc.is_measuring {
                    app.doc.is_measuring = false;
                    ctx.request_repaint();
                }
            } else if app.doc.is_measuring {
                app.doc.is_measuring = false;
            }

            // Narzędzia transformacji (Obrót, Skalowanie, Odbicie): kliknięcie otwiera dialog jeśli jest zamknięty
            if app.doc.filter_dialog.is_none()
                && matches!(
                    app.active_tool,
                    ActiveTool::Rotate | ActiveTool::Scale | ActiveTool::Flip
                )
                && app.doc.transform_dialog.is_none()
                && !space_down
                && !is_middle_down
                && is_primary_pressed
                && is_hovered
            {
                app.set_active_tool(app.active_tool);
                ctx.request_repaint();
            }

            // Obsługa narzędzia Gradient (przeciągnięcie a -> b)
            let can_gradient = app.doc.filter_dialog.is_none()
                && app.active_tool == ActiveTool::Gradient
                && !space_down
                && !is_middle_down;

            if can_gradient {
                if is_primary_down && (is_hovered || app.doc.gradient_drag_start.is_some()) {
                    if let Some(pos) = hover_pos {
                        let canvas_pos = app.doc.transform.screen_to_canvas(pos);
                        if app.doc.gradient_drag_start.is_none() {
                            app.doc.gradient_drag_start = Some(canvas_pos);
                        }
                        app.doc.gradient_drag_current = Some(canvas_pos);
                        ctx.request_repaint();
                    }
                } else if !is_primary_down && app.doc.gradient_drag_start.is_some() {
                    if let (Some(start), Some(curr)) =
                        (app.doc.gradient_drag_start, app.doc.gradient_drag_current)
                    {
                        app.apply_linear_gradient_between(start, curr);
                    }
                    app.doc.gradient_drag_start = None;
                    app.doc.gradient_drag_current = None;
                    ctx.request_repaint();
                }
            } else if app.doc.gradient_drag_start.is_some() {
                app.doc.gradient_drag_start = None;
                app.doc.gradient_drag_current = None;
            }

            // Obsługa narzędzi zaznaczania (Prostokąt / Elipsa)
            let can_select = app.doc.filter_dialog.is_none()
                && (app.active_tool == ActiveTool::SelectRect
                    || app.active_tool == ActiveTool::SelectEllipse)
                && !space_down
                && !is_middle_down;

            if can_select {
                if is_primary_down && (is_hovered || app.doc.selection_drag_start.is_some()) {
                    if let Some(pos) = hover_pos {
                        let canvas_pos = app.doc.transform.screen_to_canvas(pos);
                        if app.doc.selection_drag_start.is_none() {
                            app.doc.selection_drag_start = Some(canvas_pos);
                        }
                        app.doc.selection_drag_current = Some(canvas_pos);
                        ctx.request_repaint();
                    }
                } else if !is_primary_down && app.doc.selection_drag_start.is_some() {
                    if let (Some(start), Some(curr)) =
                        (app.doc.selection_drag_start, app.doc.selection_drag_current)
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
                        let max_x = start.x.max(end.x).min(app.doc.document.width as f32);
                        let max_y = start.y.max(end.y).min(app.doc.document.height as f32);
                        let w = (max_x - min_x).max(0.0);
                        let h = (max_y - min_y).max(0.0);

                        if w >= 1.0 && h >= 1.0 {
                            let ux = min_x.floor() as u32;
                            let uy = min_y.floor() as u32;
                            let uw = w.ceil() as u32;
                            let uh = h.ceil() as u32;

                            match app.active_tool {
                                ActiveTool::SelectRect => {
                                    app.doc.selection.select_rect(ux, uy, uw, uh);
                                    app.doc.selection_shape = SelectionShape::Rect;
                                }
                                ActiveTool::SelectEllipse => {
                                    app.doc.selection.select_ellipse(ux, uy, uw, uh);
                                    app.doc.selection_shape = SelectionShape::Ellipse;
                                }
                                _ => {}
                            }
                        } else {
                            app.doc.selection.clear();
                        }
                    }
                    app.doc.selection_drag_start = None;
                    app.doc.selection_drag_current = None;
                    ctx.request_repaint();
                }
            } else if app.doc.selection_drag_start.is_some() {
                app.doc.selection_drag_start = None;
                app.doc.selection_drag_current = None;
            }

            // Obsługa narzędzia Przesuwanie (M): przeciąganie przesuwa piksele aktywnej warstwy lub zaznaczenia
            let can_move = app.doc.filter_dialog.is_none()
                && app.active_tool == ActiveTool::Move
                && !space_down
                && !is_middle_down;

            if can_move {
                if is_primary_down && (is_hovered || app.doc.is_moving) {
                    if let Some(pos) = hover_pos {
                        let canvas_pos = app.doc.transform.screen_to_canvas(pos);
                        if !app.doc.is_moving {
                            app.start_move(canvas_pos);
                        } else if let Some(start) = app.doc.move_drag_start {
                            let dx = (canvas_pos.x - start.x).round() as i32;
                            let dy = (canvas_pos.y - start.y).round() as i32;
                            app.apply_move_offset(dx, dy);
                        }
                        ctx.request_repaint();
                    }
                } else if !is_primary_down && app.doc.is_moving {
                    app.finish_move();
                    ctx.request_repaint();
                }
            } else if app.doc.is_moving {
                app.finish_move();
                ctx.request_repaint();
            }

            // Obsługa narzędzia Kadrowanie (Shift+C): przeciąganie definiuje ramkę kadrowania
            let can_crop = app.doc.filter_dialog.is_none()
                && app.active_tool == ActiveTool::Crop
                && !space_down
                && !is_middle_down;

            if can_crop {
                if is_primary_down && (is_hovered || app.doc.crop_drag_start.is_some()) {
                    if let Some(pos) = hover_pos {
                        let canvas_pos = app.doc.transform.screen_to_canvas(pos);
                        if app.doc.crop_drag_start.is_none() {
                            app.doc.crop_drag_start = Some(canvas_pos);
                        }
                        app.doc.crop_drag_current = Some(canvas_pos);
                        ctx.request_repaint();
                    }
                } else if !is_primary_down && app.doc.crop_drag_start.is_some() {
                    if let (Some(start), Some(curr)) =
                        (app.doc.crop_drag_start, app.doc.crop_drag_current)
                    {
                        let min_x = start.x.min(curr.x).max(0.0);
                        let min_y = start.y.min(curr.y).max(0.0);
                        let max_x = start.x.max(curr.x).min(app.doc.document.width as f32);
                        let max_y = start.y.max(curr.y).min(app.doc.document.height as f32);
                        let w = (max_x - min_x).max(0.0);
                        let h = (max_y - min_y).max(0.0);

                        if w >= 1.0 && h >= 1.0 {
                            app.doc.crop_rect = Some(dziwak_core::Rect::new(
                                min_x.floor() as u32,
                                min_y.floor() as u32,
                                w.ceil() as u32,
                                h.ceil() as u32,
                            ));
                        }
                    }
                    app.doc.crop_drag_start = None;
                    app.doc.crop_drag_current = None;
                    ctx.request_repaint();
                }
            }

            // Obsługa narzędzia Zaznaczenie odręczne (F): klikane wielokąty / przeciąganie, dwuklik lub Enter zamyka
            let can_free_select = app.doc.filter_dialog.is_none()
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
                        let canvas_pos = app.doc.transform.screen_to_canvas(pos);
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
                                if let Some(&start) = app.doc.free_select_points.first() {
                                    if app.doc.free_select_points.len() >= 3 {
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
                        } else if is_primary_down && !app.doc.free_select_points.is_empty() {
                            if let Some(&last) = app.doc.free_select_points.last() {
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
                && !app.doc.free_select_points.is_empty()
            {
                app.cancel_free_select();
            }

            // Odświeżenie pamięci podręcznej obrysu zaznaczenia przy zmianie rewizji
            if app.doc.selection_outline_rev != app.doc.selection.revision {
                app.doc.selection_outline = app.doc.selection.outline_segments();
                app.doc.selection_outline_rev = app.doc.selection.revision;
            }

            // Animacja maszerujących mrówek (tylko gdy aktywne jest zaznaczenie <= 20000 odcinków lub przeciąganie)
            let has_active_selection = app.doc.selection.has_selection
                && !app.doc.selection_outline.is_empty()
                && app.doc.selection_outline.len() <= 20_000;
            let is_dragging_selection = app.doc.selection_drag_start.is_some();
            let has_crop_overlay = app.active_tool == ActiveTool::Crop
                && (app.doc.crop_rect.is_some() || app.doc.crop_drag_start.is_some());
            let has_free_select =
                app.active_tool == ActiveTool::SelectFree && !app.doc.free_select_points.is_empty();
            let has_zoom_drag =
                app.active_tool == ActiveTool::Zoom && app.doc.zoom_drag_start.is_some();
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
                if app.doc.is_moving {
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

            // Kółko myszy jak w GIMP: kółko = przewijanie góra/dół (Shift+kółko = lewo/prawo),
            // Ctrl+kółko (egui zamienia je na zoom_delta) i szczypanie touchpada = zoom wokół kursora.
            if is_hovered {
                let (scroll, zoom_delta) = ctx.input(|i| (i.smooth_scroll_delta, i.zoom_delta()));
                if scroll != egui::Vec2::ZERO {
                    app.doc.transform.pan += scroll;
                    app.doc.auto_fit = false;
                }
                // Łagodniejszy, dokładniejszy zoom: połowa kroku domyślnego egui.
                let zoom_factor = zoom_delta.powf(0.5);
                if (zoom_factor - 1.0_f32).abs() > 1e-4 {
                    if let Some(cursor_pos) = hover_pos {
                        app.doc
                            .transform
                            .zoom_around(cursor_pos, zoom_factor, 0.05, 64.0);
                        app.doc.auto_fit = false;
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

            let canvas_size = egui::vec2(
                app.doc.document.width as f32,
                app.doc.document.height as f32,
            );
            let canvas_screen_rect = egui::Rect::from_min_size(
                egui::pos2(app.doc.transform.pan.x, app.doc.transform.pan.y),
                canvas_size * app.doc.transform.zoom,
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

            // Jeśli trwa edycja tekstu i zmienił się kolor lub flaga preview_dirty, przerysuj warstwę przed kompozycją kafli
            if app.doc.text_editor.is_some()
                && app.ensure_text_editor_valid()
                && app
                    .doc
                    .text_editor
                    .as_ref()
                    .is_some_and(|e| e.preview_dirty || e.preview_color != app.brush_color)
            {
                app.update_text_preview_data();
            }

            // Aktualizacja tekstur GPU wyłącznie dla widocznych i brudnych kafli
            app.doc.tile_renderer.set_zoom(app.doc.transform.zoom);
            app.doc.tile_renderer.update_visible_dirty_tiles(
                ctx,
                &app.doc.document,
                &app.doc.transform,
                canvas_viewport_rect,
            );

            // Rysujemy skomponowane kafle dokumentu przesłane na GPU
            app.doc.tile_renderer.draw_visible_tiles(
                &canvas_painter,
                &app.doc.transform,
                app.doc.document.width,
                app.doc.document.height,
                canvas_viewport_rect,
                app.show_tile_grid,
            );

            // Rysowanie maszerujących mrówek (obrysu zaznaczenia)
            let time = ctx.input(|i| i.time);
            let phase = ((time / 0.150).floor() as i64).rem_euclid(8) as f32;

            if let (Some(start), Some(curr)) =
                (app.doc.selection_drag_start, app.doc.selection_drag_current)
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
                let max_x = start.x.max(end.x).min(app.doc.document.width as f32);
                let max_y = start.y.max(end.y).min(app.doc.document.height as f32);

                let s_min = app.doc.transform.canvas_to_screen(egui::pos2(min_x, min_y));
                let s_max = app.doc.transform.canvas_to_screen(egui::pos2(max_x, max_y));
                let drag_screen_rect = egui::Rect::from_min_max(s_min, s_max);

                match app.active_tool {
                    ActiveTool::SelectEllipse => {
                        draw_marching_ants_ellipse(&canvas_painter, drag_screen_rect, phase);
                    }
                    _ => {
                        draw_marching_ants_rect(&canvas_painter, drag_screen_rect, phase);
                    }
                }
            } else if app.doc.selection.has_selection {
                if app.doc.selection_outline_rev != app.doc.selection.revision {
                    app.doc.selection_outline = app.doc.selection.outline_segments();
                    app.doc.selection_outline_rev = app.doc.selection.revision;
                }
                let animate = app.doc.selection_outline.len() <= 20_000;
                let black_stroke = egui::Stroke::new(1.0_f32, egui::Color32::BLACK);
                let white_stroke = egui::Stroke::new(1.0_f32, egui::Color32::WHITE);
                for &(x0, y0, x1, y1) in &app.doc.selection_outline {
                    let p0 = app
                        .doc
                        .transform
                        .canvas_to_screen(egui::pos2(x0 as f32, y0 as f32));
                    let p1 = app
                        .doc
                        .transform
                        .canvas_to_screen(egui::pos2(x1 as f32, y1 as f32));
                    let seg_rect = egui::Rect::from_two_pos(p0, p1);
                    if !canvas_viewport_rect.intersects(seg_rect) {
                        continue;
                    }
                    canvas_painter.line_segment([p0, p1], black_stroke);
                    if animate {
                        canvas_painter.extend(egui::Shape::dashed_line_with_offset(
                            &[p0, p1],
                            white_stroke,
                            &[4.0_f32],
                            &[4.0_f32],
                            phase,
                        ));
                    } else {
                        canvas_painter.extend(egui::Shape::dashed_line(
                            &[p0, p1],
                            white_stroke,
                            4.0_f32,
                            4.0_f32,
                        ));
                    }
                }
            }

            // Podgląd linii gradientu podczas przeciągania
            if let (Some(start), Some(curr)) =
                (app.doc.gradient_drag_start, app.doc.gradient_drag_current)
            {
                let s_start = app.doc.transform.canvas_to_screen(start);
                let s_curr = app.doc.transform.canvas_to_screen(curr);

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
                    let r = (app.brush_size * 0.5 * app.doc.transform.zoom).max(1.0);
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
                    (app.doc.crop_drag_start, app.doc.crop_drag_current)
                {
                    let min_x = start.x.min(curr.x).max(0.0);
                    let min_y = start.y.min(curr.y).max(0.0);
                    let max_x = start.x.max(curr.x).min(app.doc.document.width as f32);
                    let max_y = start.y.max(curr.y).min(app.doc.document.height as f32);
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
                    app.doc.crop_rect
                };

                if let Some(cr) = active_crop_rect {
                    let s_min = app
                        .doc
                        .transform
                        .canvas_to_screen(egui::pos2(cr.x as f32, cr.y as f32));
                    let s_max = app.doc.transform.canvas_to_screen(egui::pos2(
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
            if app.active_tool == ActiveTool::SelectFree && !app.doc.free_select_points.is_empty() {
                let n = app.doc.free_select_points.len();
                let screen_pts: Vec<egui::Pos2> = app
                    .doc
                    .free_select_points
                    .iter()
                    .map(|&(x, y)| app.doc.transform.canvas_to_screen(egui::pos2(x, y)))
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
                let source_coord = if let Some((dx, dy)) = app.doc.clone_offset {
                    if let Some(pos) = hover_pos {
                        let cpos = app.doc.transform.screen_to_canvas(pos);
                        Some((cpos.x + dx as f32, cpos.y + dy as f32))
                    } else {
                        app.doc.clone_source
                    }
                } else {
                    app.doc.clone_source
                };

                if let Some((sx, sy)) = source_coord {
                    let sp = app.doc.transform.canvas_to_screen(egui::pos2(sx, sy));
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

            // Podgląd tekstu na żywo na płótnie (T10)
            if let (Some(start), Some(curr)) = (app.doc.text_drag_start, app.doc.text_drag_current)
            {
                let s0 = app.doc.transform.canvas_to_screen(start);
                let s1 = app.doc.transform.canvas_to_screen(curr);
                let drag_rect = egui::Rect::from_two_pos(s0, s1);
                draw_marching_ants_rect(&canvas_painter, drag_rect, phase);
            }

            if app.doc.text_editor.is_some() && app.ensure_text_editor_valid() {
                ctx.request_repaint_after(Duration::from_millis(500));

                if let Some(box_rect) = app.text_box_canvas_rect() {
                    let s_min = app.doc.transform.canvas_to_screen(box_rect.min);
                    let s_max = app.doc.transform.canvas_to_screen(box_rect.max);
                    let frame_screen_rect = egui::Rect::from_min_max(s_min, s_max);
                    draw_marching_ants_rect(&canvas_painter, frame_screen_rect, phase);
                }

                let caret_visible = ((time * 2.0).floor() as i64).rem_euclid(2) == 0;
                if caret_visible {
                    if let Some((cx, cy0, cy1)) = app.text_caret_canvas_segment() {
                        let p0 = app.doc.transform.canvas_to_screen(egui::pos2(cx, cy0));
                        let p1 = app.doc.transform.canvas_to_screen(egui::pos2(cx, cy1));
                        canvas_painter.line_segment(
                            [p0, p1],
                            egui::Stroke::new(2.5_f32, egui::Color32::BLACK),
                        );
                        canvas_painter.line_segment(
                            [p0, p1],
                            egui::Stroke::new(1.0_f32, egui::Color32::WHITE),
                        );
                    }
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

            let offset_x = app.doc.transform.pan.x - top_ruler_rect.left();
            ruler_ticks(
                offset_x,
                app.doc.transform.zoom,
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

            let offset_y = app.doc.transform.pan.y - left_ruler_rect.top();
            ruler_ticks(
                offset_y,
                app.doc.transform.zoom,
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
