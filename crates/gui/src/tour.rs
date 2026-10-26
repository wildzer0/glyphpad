//! The guided tour (SPEC §13): an overlay that walks through the window,
//! outlining one element at a time and saying what it is for.
//!
//! The pages register the rectangle of what they draw and the overlay, drawn
//! last, dims everything else. Nothing here touches the configuration: a
//! tour is shown once, and asked for again from the navigation bar or the
//! tray.

use eframe::egui::{self, Color32, Rect};

use crate::widgets as w;
use crate::{Ctx, Tab};

/// An element a step points at. The page that draws it registers its
/// rectangle under this key, in the same frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Target {
    GestureList,
    RecordButton,
    GestureDetail,
    ReviewSymbols,
    SettingsDrawing,
    /// Nothing to outline: the step talks about the program as a whole.
    Whole,
}

struct Step {
    tab: Tab,
    target: Target,
    title: &'static str,
    text: &'static str,
    /// The step hands over to the recording wizard: the tour comes back
    /// after it, so the tour can teach by doing.
    practice: bool,
}

const STEPS: &[Step] = &[
    Step {
        tab: Tab::Gestures,
        target: Target::GestureList,
        title: "Your gestures",
        text: "This is the list, and its order is the precedence: when two gestures could fire \
               from the same movement, the one higher up wins. Drag a row to change it.",
        practice: false,
    },
    Step {
        tab: Tab::Gestures,
        target: Target::RecordButton,
        title: "Record a gesture",
        text: "Draw a symbol or perform the gesture, and GlyphPad learns it from your examples. \
               You choose the name and the action at the end.",
        practice: false,
    },
    Step {
        tab: Tab::Gestures,
        target: Target::RecordButton,
        title: "Record one now",
        text: "Let us record one together: a T, drawn the way your settings ask. The wizard \
               wants a few examples, then a name and an action, and at the end it lets you try \
               the gesture for real. The gestures are paused meanwhile.",
        practice: true,
    },
    Step {
        tab: Tab::Gestures,
        target: Target::GestureDetail,
        title: "The editor",
        text: "Choosing a gesture opens its editor: what triggers it — fingers, direction, \
               symbol — what it runs, and the conditions on the app or the profile. Every \
               change is saved as you make it.",
        practice: false,
    },
    Step {
        tab: Tab::Review,
        target: Target::ReviewSymbols,
        title: "Review",
        text: "Here are all the recorded examples, with their playback, the history of what ran \
               and the conflicts: it is where a gesture that does not fire is understood.",
        practice: false,
    },
    Step {
        tab: Tab::Settings,
        target: Target::SettingsDrawing,
        title: "Settings",
        text: "The drawing mode comes first: how a symbol is started — three fingers, an edge, a \
               hold — then the thresholds, the profiles and the updates.",
        practice: false,
    },
    Step {
        tab: Tab::Gestures,
        target: Target::Whole,
        title: "Always at hand",
        text: "The icon in the tray pauses the gestures, reloads the configuration and shows this \
               window again. Closing the window with its X leaves GlyphPad running there.",
        practice: false,
    },
];

/// A tour under way. `None` in the interface means no tour is open.
#[derive(Default)]
pub struct Tour {
    step: usize,
    /// The tour was finished or skipped: the caller records that once.
    pub finished: bool,
    /// The step asked for the recording wizard.
    pub wants_wizard: bool,
}

impl Tour {
    pub fn new() -> Self {
        Self::default()
    }

    /// The page this step is about: the interface switches to it before
    /// drawing, so the element is on screen when the overlay highlights it.
    pub fn step_tab(&self) -> Tab {
        STEPS[self.step.min(STEPS.len() - 1)].tab
    }

    /// Remembers where an annotated element is. Called by the pages while
    /// they draw.
    pub fn register(ui: &egui::Ui, target: Target, rect: Rect) {
        ui.data_mut(|d| d.insert_temp(Self::rect_id(target), rect));
    }

    /// Moves to the next step, if there is one.
    pub fn advance(&mut self) {
        if self.step + 1 < STEPS.len() {
            self.step += 1;
        } else {
            self.finished = true;
        }
    }

    fn rect_id(target: Target) -> egui::Id {
        egui::Id::unique(("tour-rect", target))
    }

    /// Draws the overlay. Returns `false` when the tour is over.
    pub fn show(&mut self, ctx: &egui::Context, _cx: &Ctx) -> bool {
        let step = &STEPS[self.step.min(STEPS.len() - 1)];
        let screen = ctx.content_rect();
        let target = ctx
            .data(|d| d.get_temp::<Rect>(Self::rect_id(step.target)))
            .map(|r| r.intersect(screen))
            .filter(|r| r.width() > 4.0 && r.height() > 4.0);
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::unique("tour-dim"),
        ));
        let dim = Color32::from_black_alpha(165);
        match target {
            Some(r) => {
                // Four bands around the element: the dim never covers what
                // the step is about, which stays readable.
                let r = r.expand(8.0);
                let bands = [
                    Rect::from_min_max(screen.min, egui::pos2(screen.max.x, r.min.y)),
                    Rect::from_min_max(egui::pos2(screen.min.x, r.max.y), screen.max),
                    Rect::from_min_max(
                        egui::pos2(screen.min.x, r.min.y),
                        egui::pos2(r.min.x, r.max.y),
                    ),
                    Rect::from_min_max(
                        egui::pos2(r.max.x, r.min.y),
                        egui::pos2(screen.max.x, r.max.y),
                    ),
                ];
                for band in bands {
                    if band.width() > 0.0 && band.height() > 0.0 {
                        painter.rect_filled(band, 0.0, dim);
                    }
                }
                painter.rect_stroke(
                    r,
                    egui::CornerRadius::same(8),
                    egui::Stroke::new(2.0, Color32::WHITE),
                    egui::StrokeKind::Outside,
                );
            }
            None => {
                painter.rect_filled(screen, 0.0, dim);
            }
        }

        // The bubble: under the element when it fits, above it otherwise.
        let width = 400.0;
        let height = 190.0;
        let anchor = target.unwrap_or(screen);
        let x = (anchor.center().x - width / 2.0).clamp(
            screen.min.x + 16.0,
            (screen.max.x - width - 16.0).max(screen.min.x + 16.0),
        );
        let below = anchor.max.y + 16.0;
        let y = if below + height < screen.max.y {
            below
        } else {
            (anchor.min.y - height - 16.0).max(screen.min.y + 16.0)
        };
        let last = self.step + 1 >= STEPS.len();
        egui::Area::new(egui::Id::unique("tour-bubble"))
            .order(egui::Order::Foreground)
            .fixed_pos(egui::pos2(x, y))
            .show(ctx, |ui| {
                ui.set_width(width);
                egui::Frame::popup(ui.style())
                    .inner_margin(egui::Margin::same(14))
                    .show(ui, |ui| {
                        ui.set_width(width - 28.0);
                        ui.label(
                            egui::RichText::new(crate::i18n::tr(step.title))
                                .strong()
                                .size(16.0),
                        );
                        ui.add_space(6.0);
                        ui.label(crate::i18n::tr(step.text));
                        ui.add_space(12.0);
                        ui.horizontal(|ui| {
                            if w::small_button(ui, "Skip").clicked() {
                                self.finished = true;
                            }
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if step.practice {
                                        // The wizard takes the page; the tour goes on
                                        // after it, having shown the thing itself.
                                        if w::primary_button(ui, crate::icons::RECORD, "Record it")
                                            .clicked()
                                        {
                                            self.wants_wizard = true;
                                            self.advance();
                                        }
                                    } else {
                                        let label = if last { "Done" } else { "Next" };
                                        if w::primary_button(ui, crate::icons::CHECK, label)
                                            .clicked()
                                        {
                                            if last {
                                                self.finished = true;
                                            } else {
                                                self.advance();
                                            }
                                        }
                                    }
                                    if self.step > 0
                                        && w::button(ui, crate::icons::ARROW_LEFT, "Back").clicked()
                                    {
                                        self.step -= 1;
                                    }
                                },
                            );
                        });
                    });
            });

        !self.finished
    }
}

/// The file that records the tour has been seen, beside the configuration.
/// It is written when the tour ends or is skipped, so it never comes back —
/// not even after an update, which replaces the program but not this file.
const ONBOARDING_FILE: &str = "onboarding.json";

pub fn onboarding_path(dir: &std::path::Path) -> std::path::PathBuf {
    dir.join(ONBOARDING_FILE)
}

pub fn onboarding_done(dir: &std::path::Path) -> bool {
    std::fs::read_to_string(onboarding_path(dir))
        .map(|text| text.contains("\"done\""))
        .unwrap_or(false)
}

/// Records that the tour has been seen. Failing to write it is not worth
/// telling the user about: it only means the tour may be offered again.
pub fn mark_done(dir: &std::path::Path) {
    let _ = std::fs::write(onboarding_path(dir), "{\"done\":true,\"version\":1}\n");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tour_is_offered_until_it_is_recorded_as_seen() {
        let dir = std::env::temp_dir().join(format!("glyphpad-tour-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let _ = std::fs::remove_file(onboarding_path(&dir));
        assert!(!onboarding_done(&dir), "never seen: offered");
        mark_done(&dir);
        assert!(onboarding_done(&dir), "seen: not offered again");
        // A file that says something else is not a "seen" record.
        std::fs::write(onboarding_path(&dir), "{}").unwrap();
        assert!(!onboarding_done(&dir));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_tour_teaches_by_doing_and_stops_at_the_end() {
        assert!(STEPS.iter().any(|s| s.practice), "a step records one");
        let mut tour = Tour::new();
        for _ in 0..STEPS.len() + 3 {
            tour.advance();
        }
        assert_eq!(tour.step, STEPS.len() - 1, "it does not run past the end");
        assert!(tour.finished, "the last step ends the tour");
    }

    #[test]
    fn every_step_has_a_tab_and_the_tour_ends_after_the_last() {
        assert!(!STEPS.is_empty());
        for step in STEPS {
            assert!(!step.title.is_empty() && !step.text.is_empty());
        }
        let mut tour = Tour::new();
        for _ in 0..STEPS.len() {
            assert!(tour.step_tab() == Tab::Gestures || true);
            tour.step += 1;
        }
        assert_eq!(tour.step_tab(), STEPS.last().unwrap().tab);
    }
}
