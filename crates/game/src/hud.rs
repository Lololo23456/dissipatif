//! The interface over the world, kept discreet: the bag at the bottom, what the hands can do
//! just above it, three thin gauges (hunger, thirst, warmth) in a corner, the hour, and a
//! short message now and then. Rebuilt every frame from the game state.

use render::palette::{materials, srgb_hex};
use render::ui::{LINE, Ui};

use crate::items::{MAX_MASS, SLOTS};
use crate::needs::Needs;
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
}

fn rgba(hex: u32, alpha: f32) -> [f32; 4] {
    let [r, g, b] = srgb_hex(hex);
    [r, g, b, alpha]
}

pub fn build(ui: &mut Ui, input: &HudInput) {
    let (width, height) = ui.size();
    // One font pixel per 400 screen lines: crisp, never tiny.
    let s = (height / 400.0).round().max(1.0);
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
    let colors = materials();
    let stacks = me.inventory.stacks();
    for k in 0..SLOTS {
        let x = x0 + k as f32 * (slot + gap);
        ui.rect(x, y0, slot, slot, panel);
        if let Some(stack) = stacks.get(k) {
            let [r, g, b] = colors[stack.matter.material().id() as usize];
            let inset = 4.0 * s;
            ui.rect(
                x + inset,
                y0 + inset,
                slot - 2.0 * inset,
                slot - 2.0 * inset,
                [r, g, b, 1.0],
            );
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
    if let Some((_, matter)) = input.state.target(input.world, input.me) {
        actions.push(format!("E  Ramasser : {}", matter.name()));
    }
    if input.state.can_lay(input.me, input.selected) {
        actions.push("P  Poser le caillou".to_owned());
    }
    if input.state.can_drink(input.world, input.me) {
        actions.push("B  Boire".to_owned());
    }
    if let Some(stack) = stacks.get(input.selected)
        && stack.matter.edible()
    {
        actions.push("F  Manger".to_owned());
    }
    if !actions.is_empty() {
        let line = actions.join("     ");
        let w = Ui::text_width(&line, s);
        ui.text_shadowed((width - w) / 2.0, line_y, &line, s, ink);
    }

    // ---- Observing the anomaly ----
    if let Some(anomaly) = input.state.anomaly() {
        let feet = me.body.position;
        if anomaly.near(feet.x, feet.z, 3.0) {
            let activity = (anomaly.activity() * 100.0).round();
            let line = format!("Le tapis vit sur {activity:.0} % de sa surface");
            let w = Ui::text_width(&line, s);
            ui.text_shadowed((width - w) / 2.0, 10.0 * s, &line, s, faint);
        }
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
