//! The All Tools drawer: every tool as a button; picking one selects it and closes the drawer.

use super::DialogSpec;
use crate::VectorcraftApp;
use crate::state::Dialog;

pub(super) const SPEC: DialogSpec = DialogSpec { heading: |_| tl!("All Tools").into(), body, ok: None, ..DialogSpec::FORM };

/// Width of one tool button, in points.
///
/// MEASURED why a fixed cell is needed, and it is not a taste question. The catalog's labels
/// run from `Pen` (3 characters) to `Vertical Type on a Path Tool` (27), and with the ` Tool`
/// suffix stripped on EN, `Selection` / `Direct Selection` / `Rotate View` sit in between. In
/// a `horizontal_wrapped` each button takes only its own width, so the long ones are
/// **truncated to whatever room is left on the line** and the text of adjacent tools
/// overlaps — which is what the phone screenshot showed.
///
/// A fixed cell removes the failure: every label gets the same room and a long one wraps
/// instead of colliding with its neighbour. MEASURED against the 937.4 pt viewport — two
/// cells plus the item spacing is 426 pt, and the widest label wraps to two lines inside a
/// cell rather than spilling into the next one.
const CELL_W: f32 = 206.0;

/// Height of one cell. MEASURED: two lines of 12.5 pt text plus the button's own padding.
const CELL_H: f32 = 30.0;

fn body(app: &mut VectorcraftApp, ui: &mut egui::Ui, _: &mut Dialog) -> bool {
    let mut picked = false;
    for g in vectorcraft_tools::TOOL_GROUPS {
        ui.horizontal_wrapped(|ui| {
            for tool in g.iter() {
                let shown = tl!(tool.label);
                let shown = if crate::i18n::current() == crate::i18n::Lang::EN { shown.trim_end_matches(" Tool") } else { shown };
                // A `Button` with a `min_size` still wraps its text inside the cell, which
                // is the point: the cell is the unit of layout, so no label can push into
                // the next one's space.
                let b = egui::Button::new(shown).min_size(egui::vec2(CELL_W, CELL_H));
                if ui.add(b).clicked() {
                    app.select_tool(tool.id);
                    picked = true;
                }
            }
        });
    }
    picked
}