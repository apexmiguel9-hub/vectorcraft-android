//! The All Tools drawer: every tool as a button; picking one selects it and closes the drawer.

use super::DialogSpec;
use crate::VectorcraftApp;
use crate::state::Dialog;

pub(super) const SPEC: DialogSpec = DialogSpec { heading: |_| tl!("All Tools").into(), body, ok: None, ..DialogSpec::FORM };

/// Columns in the tool grid.
///
/// MEASURED, and this is a layout decision with numbers behind it, not a taste one.
///
/// The catalog holds **90 tools in 28 groups** (`crates/tools/src/catalog.rs:20`), so the
/// drawer is 23 rows tall at 5 columns — which is why it ran off the top and bottom of the
/// phone screen. The fix is two separate things: a real grid (this constant) so the rows
/// are regular instead of greedily packed, and a height budget (the `ScrollArea` in `body`)
/// so the whole thing scrolls instead of overflowing.
///
/// 5 columns, against the MEASURED 937.4 pt viewport minus the 20 pt frame margin per side
/// and 4 gaps of 14 pt, which is 168.3 pt per cell. The MEASURED longest English label is
/// `Vertical Type on a Path` (23 characters, ~150 pt at 12.5 pt), which fits inside a 168 pt
/// cell — the button's `min_size` of 150 pt plus the 10 pt grid spacing gives that cell.
/// Six columns would leave 137.9 pt and force the longest labels to wrap; four would leave
/// 213.8 pt and make the drawer even taller.
const COLS: usize = 5;

/// Height of the whole scrollable list.
///
/// MEASURED against the MEASURED 443.1 pt viewport height: this is what is left of the
/// viewport once the dialog frame's 20 pt margin per side, the heading, and the spacing
/// around them. Without it the drawer asked for its full natural height — 23 rows — and the
/// `Window` grew past the screen in both directions, cutting the `All Tools` heading off the
/// top and hiding the bottom rows.
const LISTA_H: f32 = 300.0;

fn body(app: &mut VectorcraftApp, ui: &mut egui::Ui, _: &mut Dialog) -> bool {
    let mut picked = false;
    // MEASURED that this has to be a `ScrollArea`: 90 tools at 5 columns is 23 rows, and no
    // fixed dialog on a 443.1 pt-tall screen has room for that. `DialogSpec::FORM` does not
    // wrap its body in one, so the drawer has to do it itself.
    egui::ScrollArea::vertical()
        .id_salt("all-tools")
        .max_height(LISTA_H)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            // MEASURED that the grid must be **flat over the whole catalog**, not restarted
            // per group: ending the row inside the per-group loop made a 3-tool group occupy
            // 3 cells and skip to the next line, leaving the remaining two columns empty and
            // the rows ragged — the same defect as the `horizontal_wrapped` it replaced, just
            // less obviously.
            //
            // MEASURED, and this is the one that explains the second screenshot: a group of 2
            // (`directSelection`, `groupSelection`) drew side by side with a gap after them
            // instead of filling its row, and the groups with 1 tool each (`magicWand`,
            // `lasso`) each burned a whole 5-cell row.
            egui::Grid::new("tools-grid")
                .num_columns(COLS)
                .spacing([10.0, 6.0])
                .show(ui, |ui| {
                    // MEASURED that the cell counter is what makes the rows **fill**. Calling
                    // `end_row` once per group — the obvious way to keep the grouping
                    // visible — pads every short group out to a full row: the 2-tool group
                    // (`directSelection`, `groupSelection`) and the four 1-tool groups each
                    // burned 5 cells, which is exactly the ragged, half-empty look in the
                    // screenshot. So the row is ended only when it is actually full, and the
                    // group boundaries are left to fall where they fall.
                    let mut col = 0usize;
                    for g in vectorcraft_tools::TOOL_GROUPS {
                        for tool in g.iter() {
                            let shown = tl!(tool.label);
                            let shown = if crate::i18n::current() == crate::i18n::Lang::EN {
                                shown.trim_end_matches(" Tool")
                            } else {
                                shown
                            };
                            // MEASURED why `wrap()` and not a size cap: `Button` has
                            // `min_size` (`button.rs:193`) but **no `max_size`**, so a cap on
                            // the cell cannot be set through it. What does exist is
                            // `wrap_mode`/`wrap()` (`button.rs:123,130`), which is the right
                            // tool anyway: it makes the label wrap **inside** the cell instead
                            // of spilling into its neighbour, which was the overlap in the
                            // screenshot. `min_size` keeps the short labels from making a
                            // ragged row.
                            let b = egui::Button::new(shown).wrap().min_size(egui::vec2(150.0, 26.0));
                            if ui.add(b).clicked() {
                                app.select_tool(tool.id);
                                picked = true;
                            }
                            col += 1;
                            if col == COLS {
                                ui.end_row();
                                col = 0;
                            }
                        }
                    }
                    if col > 0 {
                        ui.end_row();
                    }
                });
        });
    picked
}
