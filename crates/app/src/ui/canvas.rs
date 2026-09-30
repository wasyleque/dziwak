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

            // Obsługa pipety (narzędzie Pipeta lub Alt+klik w Pędzlu)
            let can_sample = app.filter_dialog.is_none()
                && (app.active_tool == ActiveTool::Eyedropper
                    || (app.active_tool == ActiveTool::Brush && alt_down))
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

            // Obsługa pędzla / gumki (LPM bez Spacji, ŚPM i bez Alt)
            let can_paint = app.filter_dialog.is_none()
                && (app.active_tool == ActiveTool::Brush || app.active_tool == ActiveTool::Eraser)
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
                        } else if let Some(p1) = app.last_stroke_pos {
                            let mut points = Vec::new();
                            app.stroke_carry =
                                interpolate_stroke(p1, p2, spacing, app.stroke_carry, &mut points);
                            for pt in points {
                                app.apply_tool_dab(pt.0, pt.1);
                            }
                            app.last_stroke_pos = Some(p2);
                        }
                        ctx.request_repaint();
                    }
                } else if !is_primary_down && app.is_painting {
                    app.is_painting = false;
                    app.last_stroke_pos = None;
                }
            } else if app.is_painting {
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

            if can_bucket && is_primary_pressed && is_hovered {
                if let Some(pos) = hover_pos {
                    let canvas_pos = app.transform.screen_to_canvas(pos);
                    app.apply_bucket_fill(canvas_pos.x, canvas_pos.y);
                    ctx.request_repaint();
                }
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

            // Animacja maszerujących mrówek (tylko gdy aktywne jest zaznaczenie lub przeciąganie)
            let has_active_selection =
                app.selection.has_selection && app.selection.bounds().is_some();
            let is_dragging_selection = app.selection_drag_start.is_some();
            if has_active_selection || is_dragging_selection {
                ctx.request_repaint_after(std::time::Duration::from_millis(150));
            }

            // Kursor myszy w zależności od trybu nawigacji i narzędzia
            if app.is_panning {
                ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
            } else if (space_down || app.active_tool == ActiveTool::Pan) && is_hovered {
                ctx.set_cursor_icon(egui::CursorIcon::Grab);
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

            // Podgląd obrysu pędzla / gumki pod kursorem
            if is_hovered
                && !alt_down
                && (app.active_tool == ActiveTool::Brush || app.active_tool == ActiveTool::Eraser)
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
