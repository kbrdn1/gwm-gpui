//! The "New worktree" form, in the card: `gwm create`'s three modes, a field
//! for each thing gwm takes, and gwm's own words when it refuses.

use gpui::{
  App, AppContext, Context, Entity, FocusHandle, Focusable, FontWeight, SharedString, actions, div, prelude::*, px,
};
use gwm_client::Create;

use crate::{
  detail::{button, error_text, pill},
  text_input::TextInput,
  theme,
  workspace::Workspace,
};

actions!(create_form, [Submit, NextField]);

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Mode {
  Issue,
  Triple,
  Name,
}

const MODES: [(Mode, &str, &str); 3] = [
  (
    Mode::Issue,
    "From an issue",
    "gwm reads the type and the description from the issue, through gh. A worktree that already has the issue is \
     selected.",
  ),
  (
    Mode::Triple,
    "Type, issue, description",
    "The type is one of gwm types; gwm checks it, and turns the description into kebab-case.",
  ),
  (Mode::Name, "Free name", "The name is the branch, as typed."),
];

/// The form's fields, kept while it is closed: a failed creation reopens it
/// as it was sent.
pub(crate) struct Form {
  pub mode: Mode,
  issue: Entity<TextInput>,
  kind: Entity<TextInput>,
  desc: Entity<TextInput>,
  name: Entity<TextInput>,
  /// Why the last submit did not reach gwm.
  pub invalid: Option<String>,
}

impl Form {
  pub fn new(cx: &mut App) -> Self {
    Self {
      mode: Mode::Issue,
      issue: cx.new(|cx| TextInput::new("594", cx)),
      kind: cx.new(|cx| TextInput::new("feat", cx)),
      desc: cx.new(|cx| TextInput::new("add login", cx)),
      name: cx.new(|cx| TextInput::new("spike-redis", cx)),
      invalid: None,
    }
  }

  /// The mode's fields, top to bottom, with their labels.
  fn fields(&self) -> Vec<(&'static str, &Entity<TextInput>)> {
    match self.mode {
      Mode::Issue => vec![("Issue", &self.issue)],
      Mode::Triple => vec![
        ("Type", &self.kind),
        ("Issue", &self.issue),
        ("Description", &self.desc),
      ],
      Mode::Name => vec![("Name", &self.name)],
    }
  }

  pub fn focus_handles(&self, cx: &App) -> Vec<FocusHandle> {
    self
      .fields()
      .into_iter()
      .map(|(_, field)| field.focus_handle(cx))
      .collect()
  }

  pub fn request(&self, cx: &App) -> Result<Create, String> {
    let text = |field: &Entity<TextInput>| field.read(cx).text().to_owned();
    match self.mode {
      Mode::Issue => Create::issue(&text(&self.issue)),
      Mode::Triple => Create::triple(&text(&self.kind), &text(&self.issue), &text(&self.desc)),
      Mode::Name => Create::name(&text(&self.name)),
    }
  }

  pub fn clear(&mut self, cx: &mut App) {
    for field in [&self.issue, &self.kind, &self.desc, &self.name] {
      field.update(cx, |field, cx| field.clear(cx));
    }
    self.invalid = None;
  }
}

/// `creating`: gwm runs, the form waits. `error`: gwm's refusal of the last one.
pub(crate) fn create_form(
  form: &Form,
  creating: bool,
  error: Option<&str>,
  cx: &mut Context<Workspace>,
) -> impl IntoElement {
  let hint = MODES
    .iter()
    .find(|(mode, ..)| *mode == form.mode)
    .map_or("", |(.., hint)| hint);
  let mut modes = div().flex().items_center().gap(px(8.));
  for (mode, label, _) in MODES {
    modes = modes.child(
      button(label, label, theme::ink(if mode == form.mode { 1. } else { 0.6 }))
        .when(mode == form.mode, |button| button.bg(theme::selected()))
        .on_click(cx.listener(move |workspace, _, window, cx| workspace.set_mode(mode, window, cx))),
    );
  }
  let mut fields = div().flex().flex_col().gap(px(8.));
  for (label, field) in form.fields() {
    fields = fields.child(
      div()
        .flex()
        .items_center()
        .child(
          div()
            .w(px(132.))
            .flex_none()
            .text_size(px(12.))
            .text_color(theme::ink(0.45))
            .child(label),
        )
        .child(div().flex_1().min_w_0().max_w(px(420.)).child(field.clone())),
    );
  }

  div()
    .key_context("CreateForm")
    .on_action(cx.listener(|workspace, _: &Submit, window, cx| workspace.submit(window, cx)))
    .on_action(cx.listener(|workspace, _: &NextField, window, cx| workspace.next_field(window, cx)))
    .size_full()
    .flex()
    .flex_col()
    .gap(px(20.))
    .p(px(28.))
    .child(
      div()
        .text_size(px(20.))
        .font_weight(FontWeight::SEMIBOLD)
        .child("New worktree"),
    )
    .child(modes)
    .child(fields)
    .child(
      div()
        .text_size(px(12.))
        .text_color(theme::ink(0.45))
        .child(SharedString::from(hint)),
    )
    .child(
      div()
        .flex()
        .items_center()
        .gap(px(8.))
        .when(creating, |row| row.child(pill("Creating…", theme::ink(0.45))))
        .when(!creating, |row| {
          row.child(
            button("create", "Create", theme::accent())
              .on_click(cx.listener(|workspace, _, window, cx| workspace.submit(window, cx))),
          )
        })
        // gwm runs on once started: closing only hides the form.
        .child(
          button("close-form", if creating { "Close" } else { "Cancel" }, theme::ink(0.9))
            .on_click(cx.listener(|workspace, _, window, cx| workspace.close_form(window, cx))),
        ),
    )
    .children(form.invalid.clone().map(error_text))
    .children(error.map(|error| error_text(error.to_owned())))
}
