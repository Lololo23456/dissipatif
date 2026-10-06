//! The interface over the world, kept discreet: the bag at the bottom, what the hands can do
//! just above it, three thin gauges (hunger, thirst, warmth) in a corner, the hour, and a
//! short message now and then. Rebuilt every frame from the game state.

use render::palette::srgb_hex;
use render::ui::{LINE, Ui};

use crate::items::{MAX_MASS, Matter, SLOTS};
use crate::needs::Needs;
use crate::objects_view::icon;
use crate::state::{GameState, PlayerId};
use world::World;

/// Everything the interface shows, gathered by the game.
pub struct HudInput<'a> {
    pub state: &'a GameState,
    pub world: &'a World,
    pub me: PlayerId,
    pub selected: usize,
    pub hour: f32,
    /// The current message and the seconds it still stays.
    pub message: Option<(&'a str, f32)>,
    /// The bag is open.
    pub bag_open: bool,
}

/// Scale of the interface: one font pixel per 400 screen lines, crisp, never tiny.
fn scale(height: f32) -> f32 {
    (height / 400.0).round().max(1.0)
}

/// Cases of the open bag: (x, y, side), left to right, top to bottom.
fn bag_cells(width: f32, height: f32) -> Vec<(f32, f32, f32)> {
    let s = scale(height);
    let side = 34.0 * s;
    let gap = 6.0 * s;
    let columns = 4;
    let total = columns as f32 * side + (columns - 1) as f32 * gap;
    let x0 = (width - total) / 2.0;
    let y0 = height * 0.22 + 24.0 * s;
    (0..SLOTS)
        .map(|k| {
            let (c, r) = (k % columns, k / columns);
            (
                x0 + c as f32 * (side + gap),
                y0 + r as f32 * (side + gap),
                side,
            )
        })
        .collect()
}

/// The case of the open bag under the screen point (x, y), if any.
pub fn bag_slot_at(width: f32, height: f32, x: f32, y: f32) -> Option<usize> {
    bag_cells(width, height)
        .iter()
        .position(|&(cx, cy, side)| (cx..cx + side).contains(&x) && (cy..cy + side).contains(&y))
}

/// Draws the icon of `matter` centred in the square (x, y, side), whole pixels only.
fn draw_icon(ui: &mut Ui, matter: Matter, x: f32, y: f32, side: f32) {
    let icon = icon(matter);
    let fit = (side * 0.8 / icon.width.max(icon.height) as f32)
        .floor()
        .max(1.0);
    let (w, h) = (icon.width as f32 * fit, icon.height as f32 * fit);
    let (ox, oy) = (x + (side - w) / 2.0, y + (side - h) / 2.0);
    for row in 0..icon.height {
        for column in 0..icon.width {
            if let Some([r, g, b]) = icon.pixels[row * icon.width + column] {
                ui.rect(
                    ox + column as f32 * fit,
                    oy + row as f32 * fit,
                    fit,
                    fit,
                    [r, g, b, 1.0],
                );
            }
        }
    }
}

/// What a matter is like, in words, from its properties (what the naturalist can tell by
/// handling it).
fn describe(matter: Matter) -> Vec<String> {
    let p = matter.properties();
    let mut lines = vec![format!("{:.2} kg", p.mass).replace('.', ",")];
    let mut says = |value: f32, words: [&str; 3]| {
        let word = if value >= 0.75 {
            words[2]
        } else if value >= 0.4 {
            words[1]
        } else if value > 0.05 {
            words[0]
        } else {
            return;
        };
        lines.push(word.to_owned());
    };
    says(p.hardness, ["tendre", "assez dur", "très dur"]);
    says(p.sharpness, ["un peu coupant", "coupant", "tranchant"]);
    says(p.flexibility, ["un peu souple", "souple", "très souple"]);
    says(p.fragility, ["un peu fragile", "fragile", "très fragile"]);
    says(p.flammability, ["brûle mal", "brûle", "brûle très bien"]);
    says(
        p.plasticity,
        ["se modèle un peu", "se modèle", "se modèle bien"],
    );
    if let Matter::Knife { uses } = matter {
        lines.push(format!("ligature : encore {uses} coupes"));
    }
    lines
}

fn rgba(hex: u32, alpha: f32) -> [f32; 4] {
    let [r, g, b] = srgb_hex(hex);
    [r, g, b, alpha]
}

pub fn build(ui: &mut Ui, input: &HudInput) {
    let (width, height) = ui.size();
    let s = scale(height);
    let ink = rgba(0xf4ead8, 0.92);
    let faint = rgba(0xf4ead8, 0.55);
    let panel = [0.02, 0.02, 0.03, 0.38];
    let Some(me) = input.state.player(input.me) else {
        return;
    };

    // ---- Bag ----
    let slot = 18.0 * s;
    let gap = 3.0 * s;
    let total = SLOTS as f32 * slot + (SLOTS - 1) as f32 * gap;
    let x0 = (width - total) / 2.0;
    let y0 = height - slot - 10.0 * s;
    let stacks = me.inventory.stacks();
    for k in 0..SLOTS {
        let x = x0 + k as f32 * (slot + gap);
        ui.rect(x, y0, slot, slot, panel);
        if let Some(stack) = stacks.get(k) {
            draw_icon(ui, stack.matter, x, y0, slot);
            if stack.count > 1 {
                let count = stack.count.to_string();
                let w = Ui::text_width(&count, s);
                ui.text_shadowed(x + slot - w - s, y0 + slot - 8.0 * s, &count, s, ink);
            }
        }
        if k == input.selected {
            ui.frame(x - s, y0 - s, slot + 2.0 * s, slot + 2.0 * s, s, ink);
        }
    }
    // Name of the selected thing, and the weight carried.
    let mut line_y = y0 - LINE * s - 2.0 * s;
    if let Some(stack) = stacks.get(input.selected) {
        let name = stack.matter.name();
        let w = Ui::text_width(name, s);
        ui.text_shadowed((width - w) / 2.0, line_y, name, s, ink);
    }
    let weight = format!(
        "{:.1} / {:.0} kg",
        me.inventory.mass().max(0.0) + 0.0,
        MAX_MASS
    )
    .replace('.', ",");
    ui.text_shadowed(x0 + total + 6.0 * s, y0 + slot - 8.0 * s, &weight, s, faint);

    // ---- What the hands can do ----
    line_y -= LINE * s + 2.0 * s;
    let mut actions = Vec::new();
    let objects = input.state.objects();
    if let Some(i) = input.state.object_in_reach(input.me) {
        let matter = objects.placed()[i].matter;
        if objects.handleable(i) {
            actions.push(format!("E  Reprendre : {}", matter.name()));
        } else {
            actions.push(format!("{} : trop chaud", matter.name()));
        }
    } else if let Some((_, matter)) = input.state.target(input.world, input.me) {
        actions.push(format!("E  Ramasser : {}", matter.name()));
    } else if let Some(matter) = input.state.ground_sample(input.world, input.me) {
        actions.push(format!("E  Prélever : {}", matter.name()));
    }
    if let Some(stack) = stacks.get(input.selected)
        && input.state.lay_point(input.world, input.me).is_some()
    {
        actions.push(format!("P  Poser : {}", stack.matter.name()));
    }
    if let Some(work) = input.state.work_plan(input.me, input.selected) {
        actions.push(format!("F  {}", work.describe()));
    }
    if input.state.would_blow(input.me) {
        actions.push("G (maintenir)  Souffler sur la braise".to_owned());
    } else if input.state.rub_target(input.me).is_some() {
        actions.push("G (maintenir)  Frotter pour une braise".to_owned());
    }
    if input.state.can_drink(input.world, input.me) {
        actions.push("B  Boire".to_owned());
    }
    if let Some(stack) = stacks.get(input.selected)
        && stack.matter.edible()
        && input.state.work_plan(input.me, input.selected).is_none()
    {
        actions.push("F  Manger".to_owned());
    }
    if !actions.is_empty() {
        let line = actions.join("     ");
        let w = Ui::text_width(&line, s);
        ui.text_shadowed((width - w) / 2.0, line_y, &line, s, ink);
    }

    // ---- Fire drill progress ----
    if me.rubbing > 0.0 {
        let share = (me.rubbing / crate::state::RUB_SECONDS).min(1.0);
        let (w, h) = (80.0 * s, 3.0 * s);
        let (x, y) = ((width - w) / 2.0, height * 0.62);
        ui.rect(x, y, w, h, [0.0, 0.0, 0.0, 0.4]);
        ui.rect(x, y, w * share, h, rgba(0xf2a35a, 0.9));
    }

    // ---- Open bag ----
    if input.bag_open {
        let cells = bag_cells(width, height);
        let (first, last) = (cells[0], cells[SLOTS - 1]);
        let detail_w = 110.0 * s;
        let (px, py) = (first.0 - 10.0 * s, first.1 - 22.0 * s);
        let help = "clic : choisir   X : jeter   Maj+X : tout jeter   Tab : fermer";
        let pw = (last.0 + last.2 - first.0 + 20.0 * s + detail_w)
            .max(Ui::text_width(help, s) + 20.0 * s);
        let ph = last.1 + last.2 + 14.0 * s - py + 28.0 * s;
        ui.rect(px, py, pw, ph, [0.03, 0.03, 0.04, 0.82]);
        ui.text_shadowed(px + 10.0 * s, py + 7.0 * s, "Sac", s, ink);
        let weight = format!(
            "{:.1} / {:.0} kg",
            me.inventory.mass().max(0.0) + 0.0,
            MAX_MASS
        )
        .replace('.', ",");
        let ww = Ui::text_width(&weight, s);
        ui.text_shadowed(px + pw - ww - 10.0 * s, py + 7.0 * s, &weight, s, faint);
        for (k, &(x, y, side)) in cells.iter().enumerate() {
            ui.rect(x, y, side, side, [0.12, 0.12, 0.13, 0.9]);
            if let Some(stack) = stacks.get(k) {
                draw_icon(ui, stack.matter, x, y, side);
                if stack.count > 1 {
                    let count = stack.count.to_string();
                    let w = Ui::text_width(&count, s);
                    ui.text_shadowed(x + side - w - 2.0 * s, y + side - 9.0 * s, &count, s, ink);
                }
            }
            if k == input.selected {
                ui.frame(x - s, y - s, side + 2.0 * s, side + 2.0 * s, s, ink);
            }
        }
        // What the selected thing is like.
        let dx = last.0 + last.2 + 14.0 * s;
        let mut dy = first.1;
        if let Some(stack) = stacks.get(input.selected) {
            ui.text_shadowed(dx, dy, stack.matter.name(), s, ink);
            dy += LINE * s + 2.0 * s;
            for line in describe(stack.matter) {
                ui.text_shadowed(dx, dy, &line, s, faint);
                dy += LINE * s;
            }
        }
        let hw = Ui::text_width(help, s);
        ui.text_shadowed(px + (pw - hw) / 2.0, py + ph - 12.0 * s, help, s, faint);
    }

    // ---- Needs ----
    needs(ui, &me.needs, 10.0 * s, height - 10.0 * s, s);

    // ---- Hour ----
    let hour = input.hour.rem_euclid(24.0);
    let clock = format!("{:02} h {:02}", hour as u32, ((hour.fract()) * 60.0) as u32);
    let w = Ui::text_width(&clock, s);
    ui.text_shadowed(width - w - 10.0 * s, 10.0 * s, &clock, s, faint);

    // ---- Message ----
    if let Some((text, left)) = input.message {
        let alpha = (left / 0.5).min(1.0);
        let scale = s * 1.5;
        let w = Ui::text_width(text, scale);
        let color = [ink[0], ink[1], ink[2], ink[3] * alpha];
        ui.text_shadowed((width - w) / 2.0, height * 0.18, text, scale, color);
    }
}

/// Three thin gauges, bottom left: label, then a bar that empties.
fn needs(ui: &mut Ui, needs: &Needs, x: f32, bottom: f32, s: f32) {
    let label = rgba(0xf4ead8, 0.75);
    let rows = [
        ("Faim", needs.food, 0xd9a24a),
        ("Soif", needs.water, 0x6fa8d6),
        ("Chaleur", needs.warmth, 0xd9735a),
    ];
    let bar_x = x + 44.0 * s;
    let bar_w = 50.0 * s;
    let mut y = bottom - rows.len() as f32 * 10.0 * s;
    if needs.sick > 0.0 {
        ui.text_shadowed(x, y - 11.0 * s, "Malade", s, rgba(0xa8c46a, 0.9));
    }
    for (name, value, color) in rows {
        ui.text_shadowed(x, y, name, s, label);
        ui.rect(bar_x, y + 2.0 * s, bar_w, 3.0 * s, [0.0, 0.0, 0.0, 0.35]);
        // A nearly empty gauge fades a little: noticed, without alarm.
        let alpha = if value < 0.2 { 0.6 } else { 0.85 };
        ui.rect(
            bar_x,
            y + 2.0 * s,
            bar_w * value.clamp(0.0, 1.0),
            3.0 * s,
            rgba(color, alpha),
        );
        y += 10.0 * s;
    }
}
