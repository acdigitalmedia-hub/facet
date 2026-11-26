//! The map editor: paint walls (cliffs or water), place and order checkpoints, pick the
//! setting and size, see the enemy route live, playtest and save (custom maps go to the
//! user folder, see `MapDef::save_all`).

use bevy_egui::egui::{self, Align2, Color32, CornerRadius, FontId, Margin, Pos2, Rect, RichText, Sense, Shape, Stroke, StrokeKind, Vec2 as V2};
use facet_core::data::MapDef;
use facet_core::path::{full_route, Cell};

use crate::ui::{alpha, heading, small, BAD, DIM, GOLD, GOOD, INFO, PANEL_BG, PANEL_W, TEXT};

const THEMES: [(&str, &str); 5] = [("meadow", "Meadow"), ("autumn", "Autumn"), ("ash", "Ash"), ("marsh", "Marsh"), ("snow", "Snow")];
const MIN_SIZE: i32 = 15;
const MAX_SIZE: i32 = 51;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tool {
    Wall,
    Erase,
    Checkpoint,
}

/// What the editor asks the app to do.
pub enum EditorOut {
    Exit,
    Playtest(MapDef),
}

pub struct Editor {
    map: MapDef,
    /// Wall cells (row-major).
    cells: Vec<bool>,
    /// Name of the saved map being edited (None for a new map).
    original: Option<String>,
    tool: Tool,
    /// While dragging: whether we're adding (true) or clearing (false) walls, and the last cell.
    painting: Option<(bool, Cell)>,
    route: Option<Vec<Cell>>,
    dirty: bool,
    msg: Option<(String, Color32, f32)>,
    confirm_delete: bool,
    confirm_leave: bool,
}

impl Editor {
    pub fn from_map(m: &MapDef, existing: bool) -> Editor {
        let mut cells = vec![false; (m.w * m.h) as usize];
        for y in 0..m.h {
            for x in 0..m.w {
                cells[(y * m.w + x) as usize] = m.is_wall(x, y);
            }
        }
        let mut e = Editor {
            map: m.clone(),
            cells,
            original: existing.then(|| m.name.clone()),
            tool: Tool::Wall,
            painting: None,
            route: None,
            dirty: false,
            msg: None,
            confirm_delete: false,
            confirm_leave: false,
        };
        e.sync();
        e
    }

    pub fn blank() -> Editor {
        let m = MapDef {
            name: "New Map".into(),
            desc: "A map of my own.".into(),
            theme: "meadow".into(),
            w: 31,
            h: 31,
            waypoints: vec![(2, 2), (28, 28)],
            walls: Vec::new(),
            wall_style: "rock".into(),
            custom: true,
        };
        let mut e = Editor::from_map(&m, false);
        e.dirty = true;
        e
    }

    fn wall(&self, c: Cell) -> bool {
        c.0 >= 0 && c.1 >= 0 && c.0 < self.map.w && c.1 < self.map.h && self.cells[(c.1 * self.map.w + c.0) as usize]
    }

    fn set_wall(&mut self, c: Cell, on: bool) {
        if c.0 < 0 || c.1 < 0 || c.0 >= self.map.w || c.1 >= self.map.h {
            return;
        }
        if on && self.map.waypoints.contains(&c) {
            return;
        }
        let i = (c.1 * self.map.w + c.0) as usize;
        if self.cells[i] != on {
            self.cells[i] = on;
            self.dirty = true;
        }
    }

    /// Rebuilds wall rectangles from the painted cells and recomputes the route.
    fn sync(&mut self) {
        self.map.walls = rects(&self.cells, self.map.w, self.map.h);
        let (w, h) = (self.map.w, self.map.h);
        let cells = &self.cells;
        self.route = if self.map.waypoints.len() >= 2 {
            full_route(w, h, &self.map.waypoints, &|x, y| cells[(y * w + x) as usize])
        } else {
            None
        };
    }

    fn resize(&mut self, w: i32, h: i32) {
        if w == self.map.w && h == self.map.h {
            return;
        }
        let mut cells = vec![false; (w * h) as usize];
        for y in 0..h.min(self.map.h) {
            for x in 0..w.min(self.map.w) {
                cells[(y * w + x) as usize] = self.cells[(y * self.map.w + x) as usize];
            }
        }
        self.cells = cells;
        self.map.w = w;
        self.map.h = h;
        self.map.waypoints.retain(|&(x, y)| x < w && y < h);
        self.dirty = true;
    }

    fn flash(&mut self, text: impl Into<String>, col: Color32) {
        self.msg = Some((text.into(), col, 3.0));
    }

    /// The map as it would be saved.
    pub fn current(&self) -> MapDef {
        let mut m = self.map.clone();
        m.walls = rects(&self.cells, m.w, m.h);
        m.name = m.name.trim().to_string();
        m
    }

    fn save(&mut self) -> Result<Vec<MapDef>, String> {
        let m = self.current();
        if let Some(p) = m.problem() {
            return Err(p);
        }
        let (mut maps, _) = MapDef::load_all();
        let clash = maps.iter().any(|o| o.name == m.name && Some(&o.name) != self.original.as_ref());
        if clash {
            return Err(format!("A map called \"{}\" already exists", m.name));
        }
        let locked = |o: &MapDef| !o.custom && !MapDef::builtins_editable();
        match self.original.as_ref().and_then(|n| maps.iter().position(|o| &o.name == n)) {
            // A built-in map in an installed game: keep it, save the edit as the player's own copy.
            Some(i) if locked(&maps[i]) => {
                if maps[i].name == m.name {
                    return Err("Built-in maps can't be changed: give it a new name to save your own copy".into());
                }
                maps.push(MapDef { custom: true, ..m.clone() });
            }
            Some(i) => maps[i] = MapDef { custom: maps[i].custom, ..m.clone() },
            None => maps.push(MapDef { custom: true, ..m.clone() }),
        }
        MapDef::save_all(&maps)?;
        self.original = Some(m.name.clone());
        self.dirty = false;
        Ok(maps)
    }

    fn delete(&mut self) -> Result<Vec<MapDef>, String> {
        let Some(name) = self.original.clone() else { return Err("This map isn't saved yet".into()) };
        let (mut maps, _) = MapDef::load_all();
        if maps.iter().any(|m| m.name == name && !m.custom) && !MapDef::builtins_editable() {
            return Err("Built-in maps can't be deleted".into());
        }
        maps.retain(|m| m.name != name);
        if maps.is_empty() {
            return Err("Can't delete the last map".into());
        }
        MapDef::save_all(&maps)?;
        Ok(maps)
    }
}

/// Merges painted cells into rectangles: runs along each row, stacked when identical.
fn rects(cells: &[bool], w: i32, h: i32) -> Vec<[i32; 4]> {
    let mut out: Vec<[i32; 4]> = Vec::new();
    // Index into `out` of the rectangle that ended on the previous row, keyed by (x, width).
    let mut open: Vec<((i32, i32), usize)> = Vec::new();
    for y in 0..h {
        let mut runs = Vec::new();
        let mut x = 0;
        while x < w {
            if cells[(y * w + x) as usize] {
                let x0 = x;
                while x < w && cells[(y * w + x) as usize] {
                    x += 1;
                }
                runs.push((x0, x - x0));
            } else {
                x += 1;
            }
        }
        let mut next = Vec::new();
        for run in runs {
            match open.iter().find(|(k, _)| *k == run) {
                Some(&(_, i)) => {
                    out[i][3] += 1;
                    next.push((run, i));
                }
                None => {
                    out.push([run.0, y, run.1, 1]);
                    next.push((run, out.len() - 1));
                }
            }
        }
        open = next;
    }
    out
}

fn ground(theme: &str) -> Color32 {
    match theme {
        "autumn" => Color32::from_rgb(110, 80, 40),
        "ash" => Color32::from_rgb(58, 50, 50),
        "marsh" => Color32::from_rgb(55, 78, 50),
        "snow" => Color32::from_rgb(205, 212, 225),
        _ => Color32::from_rgb(70, 100, 45),
    }
}

/// Draws the editor; returns a request for the app.
pub fn show(ctx: &egui::Context, ed: &mut Editor, maps: &[MapDef], dt: f32, clicked: &mut bool) -> (Option<EditorOut>, Option<Vec<MapDef>>) {
    let screen = ctx.content_rect();
    let mut out = None;
    let mut new_maps = None;
    if let Some((_, _, t)) = &mut ed.msg {
        *t -= dt;
        if *t <= 0.0 {
            ed.msg = None;
        }
    }
    let bg = ctx.layer_painter(egui::LayerId::new(egui::Order::Background, "editor_bg".into()));
    bg.rect_filled(screen, CornerRadius::ZERO, Color32::from_rgb(12, 11, 10));

    // ---- Canvas ----
    let area = Rect::from_min_max(screen.min + V2::new(20.0, 20.0), Pos2::new(screen.max.x - PANEL_W - 20.0, screen.max.y - 20.0));
    let (w, h) = (ed.map.w, ed.map.h);
    let cs = (area.width() / w as f32).min(area.height() / h as f32).floor().max(4.0);
    let size = V2::new(cs * w as f32, cs * h as f32);
    let origin = area.center() - size / 2.0;
    let board = Rect::from_min_size(origin, size);
    let cell_at = |p: Pos2| {
        let v = (p - origin) / cs;
        let c = (v.x.floor() as i32, v.y.floor() as i32);
        (c.0 >= 0 && c.1 >= 0 && c.0 < w && c.1 < h).then_some(c)
    };
    let center = |c: Cell| origin + V2::new(c.0 as f32 + 0.5, c.1 as f32 + 0.5) * cs;
    let mut changed = false;

    egui::Area::new("editor_canvas".into()).fixed_pos(board.min).show(ctx, |ui| {
        let (rect, resp) = ui.allocate_exact_size(size, Sense::click_and_drag());
        let p = ui.painter_at(rect);
        p.rect_filled(rect, CornerRadius::ZERO, ground(&ed.map.theme));
        let water = ed.map.wall_style == "water";
        let wall_col = if water { Color32::from_rgb(40, 90, 115) } else { Color32::from_rgb(120, 112, 104) };
        for y in 0..h {
            for x in 0..w {
                if ed.wall((x, y)) {
                    let r = Rect::from_min_size(origin + V2::new(x as f32, y as f32) * cs, V2::splat(cs));
                    p.rect_filled(r, CornerRadius::ZERO, wall_col);
                }
            }
        }
        let grid = alpha(Color32::BLACK, 0.18);
        for x in 0..=w {
            let px = origin.x + x as f32 * cs;
            p.line_segment([Pos2::new(px, origin.y), Pos2::new(px, origin.y + size.y)], Stroke::new(1.0, grid));
        }
        for y in 0..=h {
            let py = origin.y + y as f32 * cs;
            p.line_segment([Pos2::new(origin.x, py), Pos2::new(origin.x + size.x, py)], Stroke::new(1.0, grid));
        }
        if let Some(route) = &ed.route {
            let pts: Vec<Pos2> = route.iter().map(|&c| center(c)).collect();
            p.add(Shape::line(pts, Stroke::new((cs * 0.3).max(2.0), alpha(Color32::from_rgb(200, 160, 100), 0.9))));
        }
        let n = ed.map.waypoints.len();
        for (k, &c) in ed.map.waypoints.iter().enumerate() {
            let (label, col) = match k {
                0 => ("S".to_string(), Color32::from_rgb(190, 140, 255)),
                _ if k == n - 1 => ("E".to_string(), BAD),
                _ => (k.to_string(), GOLD),
            };
            let pc = center(c);
            p.circle_filled(pc, (cs * 0.55).max(7.0), Color32::from_rgba_unmultiplied(0, 0, 0, 200));
            p.circle_stroke(pc, (cs * 0.55).max(7.0), Stroke::new(2.0, col));
            p.text(pc, Align2::CENTER_CENTER, label, FontId::new((cs * 0.6).clamp(9.0, 16.0), heading()), col);
        }
        if let Some(hp) = resp.hover_pos()
            && let Some(c) = cell_at(hp)
        {
            let r = Rect::from_min_size(origin + V2::new(c.0 as f32, c.1 as f32) * cs, V2::splat(cs));
            p.rect_stroke(r, CornerRadius::ZERO, Stroke::new(2.0, alpha(Color32::WHITE, 0.8)), StrokeKind::Inside);
            p.text(
                Pos2::new(rect.max.x - 6.0, rect.max.y - 6.0),
                Align2::RIGHT_BOTTOM,
                format!("{}, {}", c.0, c.1),
                FontId::proportional(13.0),
                alpha(Color32::WHITE, 0.8),
            );
        }

        // Editing.
        let pointer = ui.input(|i| i.pointer.interact_pos());
        let primary_down = ui.input(|i| i.pointer.primary_down());
        let secondary_down = ui.input(|i| i.pointer.secondary_down());
        if let Some(pos) = pointer
            && resp.hovered()
            && let Some(c) = cell_at(pos)
        {
            if secondary_down {
                // Right button always erases.
                ed.set_wall(c, false);
                if ed.map.waypoints.contains(&c) {
                    ed.map.waypoints.retain(|&w| w != c);
                    ed.dirty = true;
                }
                changed = true;
            } else if primary_down {
                match ed.tool {
                    Tool::Wall | Tool::Erase => {
                        let (on, last) = *ed.painting.get_or_insert_with(|| (ed.tool == Tool::Wall && !ed.cells[(c.1 * w + c.0) as usize], c));
                        // Fill in cells skipped by a fast drag.
                        let steps = (c.0 - last.0).abs().max((c.1 - last.1).abs()).max(1);
                        for s in 0..=steps {
                            let t = s as f32 / steps as f32;
                            let cc = ((last.0 as f32 + (c.0 - last.0) as f32 * t).round() as i32, (last.1 as f32 + (c.1 - last.1) as f32 * t).round() as i32);
                            ed.set_wall(cc, on);
                            if !on && ed.tool == Tool::Erase && ed.map.waypoints.contains(&cc) {
                                ed.map.waypoints.retain(|&w| w != cc);
                                ed.dirty = true;
                            }
                        }
                        ed.painting = Some((on, c));
                        changed = true;
                    }
                    Tool::Checkpoint => {
                        if resp.clicked() || resp.drag_started() {
                            if ed.map.waypoints.contains(&c) {
                                ed.map.waypoints.retain(|&w| w != c);
                            } else if ed.wall(c) {
                                ed.flash("Checkpoints can't go on walls", BAD);
                            } else {
                                // Added before the exit, so the exit stays last.
                                let len = ed.map.waypoints.len();
                                let at = if len < 2 { len } else { len - 1 };
                                ed.map.waypoints.insert(at, c);
                            }
                            ed.dirty = true;
                            changed = true;
                        }
                    }
                }
            }
        }
        if !primary_down {
            ed.painting = None;
        }
    });

    // ---- Side panel ----
    let frame = egui::Frame::new().fill(PANEL_BG).inner_margin(Margin::symmetric(18, 14)).stroke(Stroke::new(1.0, alpha(GOLD, 0.35)));
    egui::Area::new("editor_panel".into()).fixed_pos(Pos2::new(screen.max.x - PANEL_W, screen.min.y)).show(ctx, |ui| {
        frame.show(ui, |ui| {
            let inner = PANEL_W - 36.0;
            ui.set_width(inner);
            ui.set_min_height(screen.height() - 28.0);
            egui::ScrollArea::vertical().max_height(screen.height() - 40.0).show(ui, |ui| {
                ui.label(RichText::new("Map Editor").family(heading()).size(30.0).color(GOLD));

                // Which map.
                let current = ed.original.clone().unwrap_or_else(|| "New map (unsaved)".into());
                let mut pick: Option<Option<usize>> = None;
                egui::ComboBox::from_id_salt("editor_pick").width(inner - 10.0).selected_text(current).show_ui(ui, |ui| {
                    if ui.selectable_label(false, "+ New map").clicked() {
                        pick = Some(None);
                    }
                    for (i, m) in maps.iter().enumerate() {
                        if ui.selectable_label(ed.original.as_deref() == Some(m.name.as_str()), &m.name).clicked() {
                            pick = Some(Some(i));
                        }
                    }
                });
                if let Some(choice) = pick {
                    *ed = match choice {
                        None => Editor::blank(),
                        Some(i) => Editor::from_map(&maps[i], true),
                    };
                    *clicked = true;
                    changed = true;
                }

                ui.add_space(6.0);
                ui.label(RichText::new("Name").size(14.0).color(DIM));
                if ui.add(egui::TextEdit::singleline(&mut ed.map.name).desired_width(inner)).changed() {
                    ed.dirty = true;
                }
                ui.label(RichText::new("Description").size(14.0).color(DIM));
                if ui.add(egui::TextEdit::multiline(&mut ed.map.desc).desired_width(inner).desired_rows(2)).changed() {
                    ed.dirty = true;
                }

                ui.add_space(4.0);
                ui.label(RichText::new("Setting").size(14.0).color(DIM));
                ui.horizontal_wrapped(|ui| {
                    for (id, name) in THEMES {
                        if ui.selectable_label(ed.map.theme == id, RichText::new(name).size(14.0)).clicked() {
                            ed.map.theme = id.into();
                            ed.dirty = true;
                        }
                    }
                });
                ui.label(RichText::new("Walls are").size(14.0).color(DIM));
                ui.horizontal(|ui| {
                    for (id, name) in [("rock", "Cliffs"), ("water", "Water")] {
                        if ui.selectable_label(ed.map.wall_style == id, RichText::new(name).size(14.0)).clicked() {
                            ed.map.wall_style = id.into();
                            ed.dirty = true;
                        }
                    }
                });
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    let (mut nw, mut nh) = (ed.map.w, ed.map.h);
                    ui.label(RichText::new("Size").size(14.0).color(DIM));
                    ui.add(egui::DragValue::new(&mut nw).range(MIN_SIZE..=MAX_SIZE).speed(0.2).prefix("W "));
                    ui.add(egui::DragValue::new(&mut nh).range(MIN_SIZE..=MAX_SIZE).speed(0.2).prefix("H "));
                    if (nw, nh) != (ed.map.w, ed.map.h) {
                        ed.resize(nw, nh);
                        changed = true;
                    }
                });

                ui.add_space(8.0);
                ui.label(RichText::new("Tool").family(heading()).size(17.0).color(GOLD));
                ui.horizontal(|ui| {
                    for (t, name, tip) in [
                        (Tool::Wall, "Walls", "Drag to paint walls (start on a wall to clear instead)."),
                        (Tool::Erase, "Erase", "Drag to clear walls and remove checkpoints."),
                        (Tool::Checkpoint, "Checkpoints", "Click to add a checkpoint before the exit; click one to remove it."),
                    ] {
                        if ui.selectable_label(ed.tool == t, RichText::new(name).size(15.0)).on_hover_text(tip).clicked() {
                            ed.tool = t;
                        }
                    }
                });
                small(ui, "Right-click always erases.", DIM);

                ui.add_space(8.0);
                ui.label(RichText::new("Checkpoints").family(heading()).size(17.0).color(GOLD));
                let n = ed.map.waypoints.len();
                let mut action: Option<(usize, i32)> = None;
                for k in 0..n {
                    let (x, y) = ed.map.waypoints[k];
                    let label = match k {
                        0 => "Start".to_string(),
                        _ if k == n - 1 => "Exit".to_string(),
                        _ => format!("Checkpoint {k}"),
                    };
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(format!("{label}  ({x}, {y})")).size(14.0));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("×").on_hover_text("Remove").clicked() {
                                action = Some((k, 0));
                            }
                            if ui.add_enabled(k + 1 < n, egui::Button::new("▼").small()).on_hover_text("Move later").clicked() {
                                action = Some((k, 1));
                            }
                            if ui.add_enabled(k > 0, egui::Button::new("▲").small()).on_hover_text("Move earlier").clicked() {
                                action = Some((k, -1));
                            }
                        });
                    });
                }
                if let Some((k, a)) = action {
                    match a {
                        0 => {
                            ed.map.waypoints.remove(k);
                        }
                        -1 => ed.map.waypoints.swap(k, k - 1),
                        _ => ed.map.waypoints.swap(k, k + 1),
                    }
                    ed.dirty = true;
                    changed = true;
                }

                ui.add_space(8.0);
                match (ed.current().problem(), &ed.route) {
                    (Some(p), _) => small(ui, p, BAD),
                    (None, Some(r)) => small(ui, format!("Route length {} cells. Looks playable!", r.len()), GOOD),
                    _ => {}
                }
                if let Some((m, col, _)) = &ed.msg {
                    small(ui, m.clone(), *col);
                }

                ui.add_space(8.0);
                let w2 = (inner - 8.0) / 2.0;
                ui.horizontal(|ui| {
                    let ok = ed.current().problem().is_none();
                    if ui
                        .add_enabled(ok, egui::Button::new(RichText::new("Playtest").family(heading()).size(16.0)).min_size(V2::new(w2, 34.0)))
                        .on_hover_text("Play this map on Normal without saving. Quitting returns here.")
                        .clicked()
                    {
                        out = Some(EditorOut::Playtest(ed.current()));
                        *clicked = true;
                    }
                    if ui
                        .add_enabled(ok && ed.dirty, egui::Button::new(RichText::new("Save").family(heading()).size(16.0)).min_size(V2::new(w2, 34.0)))
                        .on_hover_text("Saves to your maps (the previous file is kept as maps.json.bak).")
                        .clicked()
                    {
                        match ed.save() {
                            Ok(m) => {
                                ed.flash("Saved!", GOOD);
                                new_maps = Some(m);
                            }
                            Err(e) => ed.flash(e, BAD),
                        }
                        *clicked = true;
                    }
                });
                ui.horizontal(|ui| {
                    if ui.add(egui::Button::new(RichText::new("Clear walls").size(14.0)).min_size(V2::new(w2, 28.0))).clicked() {
                        ed.cells.iter_mut().for_each(|c| *c = false);
                        ed.dirty = true;
                        changed = true;
                        *clicked = true;
                    }
                    let del = egui::Button::new(RichText::new(if ed.confirm_delete { "Really delete?" } else { "Delete map" }).size(14.0).color(BAD))
                        .min_size(V2::new(w2, 28.0));
                    if ui.add_enabled(ed.original.is_some(), del).clicked() {
                        *clicked = true;
                        if ed.confirm_delete {
                            match ed.delete() {
                                Ok(m) => {
                                    *ed = Editor::blank();
                                    ed.flash("Map deleted", INFO);
                                    new_maps = Some(m);
                                }
                                Err(e) => ed.flash(e, BAD),
                            }
                            ed.confirm_delete = false;
                        } else {
                            ed.confirm_delete = true;
                        }
                    }
                });
                ui.add_space(6.0);
                let leave = if ed.confirm_leave { "Leave without saving?" } else { "Back to Title" };
                if ui.add(egui::Button::new(RichText::new(leave).family(heading()).size(16.0)).min_size(V2::new(inner, 32.0))).clicked() {
                    *clicked = true;
                    if ed.dirty && !ed.confirm_leave {
                        ed.confirm_leave = true;
                    } else {
                        out = Some(EditorOut::Exit);
                    }
                }
                ui.add_space(8.0);
                small(ui, "Tips: enemies walk Start → 1 → 2 → … → Exit along the shortest open path. Players can't build on checkpoints or walls. Long routes and choke points make good mazes.", DIM);
                let _ = TEXT;
            });
        });
    });
    if changed {
        ed.sync();
        ed.confirm_delete = false;
    }
    (out, new_maps)
}
