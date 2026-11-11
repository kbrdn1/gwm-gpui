//! A one-line text field: GPUI has none built in. Trimmed from GPUI's own
//! `examples/input.rs` (Apache-2.0, see NOTICE) to what the create form needs:
//! typing and IME through [`EntityInputHandler`], backspace and delete, the
//! caret moved by the arrows, home and end, a click, select all, and paste.
//! The caret moves by `char`, not by grapheme: names and numbers are ASCII.

use std::ops::Range;

use gpui::{
  App, Bounds, Context, CursorStyle, ElementId, ElementInputHandler, Entity, EntityInputHandler, FocusHandle,
  Focusable, GlobalElementId, LayoutId, MouseButton, MouseDownEvent, PaintQuad, Pixels, ShapedLine, SharedString,
  Style, TextRun, UTF16Selection, UnderlineStyle, Window, actions, div, fill, point, prelude::*, px, relative, size,
};

use crate::theme;

actions!(
  text_input,
  [Backspace, Delete, Left, Right, Home, End, SelectAll, Paste]
);

pub(crate) struct TextInput {
  focus: FocusHandle,
  content: SharedString,
  placeholder: SharedString,
  selected_range: Range<usize>,
  selection_reversed: bool,
  marked_range: Option<Range<usize>>,
  last_layout: Option<ShapedLine>,
  last_bounds: Option<Bounds<Pixels>>,
}

impl TextInput {
  pub fn new(placeholder: &'static str, cx: &mut Context<Self>) -> Self {
    Self {
      focus: cx.focus_handle(),
      content: SharedString::default(),
      placeholder: placeholder.into(),
      selected_range: 0..0,
      selection_reversed: false,
      marked_range: None,
      last_layout: None,
      last_bounds: None,
    }
  }

  pub fn text(&self) -> &str {
    &self.content
  }

  pub fn clear(&mut self, cx: &mut Context<Self>) {
    self.content = SharedString::default();
    self.selected_range = 0..0;
    self.selection_reversed = false;
    self.marked_range = None;
    cx.notify();
  }

  fn left(&mut self, _: &Left, window: &mut Window, cx: &mut Context<Self>) {
    if self.selected_range.is_empty() {
      self.move_to(self.previous_boundary(self.cursor_offset()), window, cx);
    } else {
      self.move_to(self.selected_range.start, window, cx);
    }
  }

  fn right(&mut self, _: &Right, window: &mut Window, cx: &mut Context<Self>) {
    if self.selected_range.is_empty() {
      self.move_to(self.next_boundary(self.selected_range.end), window, cx);
    } else {
      self.move_to(self.selected_range.end, window, cx);
    }
  }

  fn home(&mut self, _: &Home, window: &mut Window, cx: &mut Context<Self>) {
    self.move_to(0, window, cx);
  }

  fn end(&mut self, _: &End, window: &mut Window, cx: &mut Context<Self>) {
    self.move_to(self.content.len(), window, cx);
  }

  fn select_all(&mut self, _: &SelectAll, window: &mut Window, cx: &mut Context<Self>) {
    self.selected_range = 0..self.content.len();
    self.selection_reversed = false;
    window.notify_text_selection_changed(&self.focus);
    cx.notify();
  }

  fn backspace(&mut self, _: &Backspace, window: &mut Window, cx: &mut Context<Self>) {
    if self.selected_range.is_empty() {
      let previous = self.previous_boundary(self.cursor_offset());
      if previous == self.cursor_offset() {
        return;
      }
      self.selected_range = previous..self.cursor_offset();
    }
    self.replace_text_in_range(None, "", window, cx);
  }

  fn delete(&mut self, _: &Delete, window: &mut Window, cx: &mut Context<Self>) {
    if self.selected_range.is_empty() {
      let next = self.next_boundary(self.cursor_offset());
      if next == self.cursor_offset() {
        return;
      }
      self.selected_range = self.cursor_offset()..next;
    }
    self.replace_text_in_range(None, "", window, cx);
  }

  fn paste(&mut self, _: &Paste, window: &mut Window, cx: &mut Context<Self>) {
    if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
      self.replace_text_in_range(None, &text.replace('\n', " "), window, cx);
    }
  }

  fn on_mouse_down(&mut self, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
    let offset = match (&self.last_bounds, &self.last_layout) {
      (Some(bounds), Some(line)) if !self.content.is_empty() => {
        line.closest_index_for_x(event.position.x - bounds.left())
      }
      _ => 0,
    };
    self.move_to(offset, window, cx);
  }

  fn move_to(&mut self, offset: usize, window: &mut Window, cx: &mut Context<Self>) {
    self.selected_range = offset..offset;
    self.selection_reversed = false;
    window.notify_text_selection_changed(&self.focus);
    cx.notify();
  }

  fn cursor_offset(&self) -> usize {
    if self.selection_reversed {
      self.selected_range.start
    } else {
      self.selected_range.end
    }
  }

  fn previous_boundary(&self, offset: usize) -> usize {
    self.content[..offset]
      .char_indices()
      .next_back()
      .map_or(0, |(index, _)| index)
  }

  fn next_boundary(&self, offset: usize) -> usize {
    self.content[offset..]
      .chars()
      .next()
      .map_or(offset, |next| offset + next.len_utf8())
  }

  fn offset_from_utf16(&self, offset: usize) -> usize {
    let mut utf8 = 0;
    let mut utf16 = 0;
    for ch in self.content.chars() {
      if utf16 >= offset {
        break;
      }
      utf16 += ch.len_utf16();
      utf8 += ch.len_utf8();
    }
    utf8
  }

  fn offset_to_utf16(&self, offset: usize) -> usize {
    self.content[..offset].encode_utf16().count()
  }

  fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
    self.offset_to_utf16(range.start)..self.offset_to_utf16(range.end)
  }

  fn range_from_utf16(&self, range: &Range<usize>) -> Range<usize> {
    self.offset_from_utf16(range.start)..self.offset_from_utf16(range.end)
  }

  /// The range the platform edits: its own, else the marked text, else the selection.
  fn target(&self, range_utf16: Option<Range<usize>>) -> Range<usize> {
    range_utf16
      .as_ref()
      .map(|range| self.range_from_utf16(range))
      .or(self.marked_range.clone())
      .unwrap_or(self.selected_range.clone())
  }
}

impl EntityInputHandler for TextInput {
  fn text_for_range(
    &mut self,
    range_utf16: Range<usize>,
    actual_range: &mut Option<Range<usize>>,
    _: &mut Window,
    _: &mut Context<Self>,
  ) -> Option<String> {
    let range = self.range_from_utf16(&range_utf16);
    actual_range.replace(self.range_to_utf16(&range));
    Some(self.content[range].to_owned())
  }

  fn selected_text_range(&mut self, _: bool, _: &mut Window, _: &mut Context<Self>) -> Option<UTF16Selection> {
    Some(UTF16Selection {
      range: self.range_to_utf16(&self.selected_range),
      reversed: self.selection_reversed,
    })
  }

  fn set_selected_text_range(&mut self, range_utf16: Range<usize>, window: &mut Window, cx: &mut Context<Self>) {
    self.selected_range = self.range_from_utf16(&range_utf16);
    self.selection_reversed = false;
    window.notify_text_selection_changed(&self.focus);
    cx.notify();
  }

  fn text_length_utf16(&mut self, _: &mut Window, _: &mut Context<Self>) -> Option<usize> {
    Some(self.content.encode_utf16().count())
  }

  fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
    self.marked_range.as_ref().map(|range| self.range_to_utf16(range))
  }

  fn unmark_text(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    self.marked_range = None;
    window.notify_text_input_changed(&self.focus);
    cx.notify();
  }

  fn replace_text_in_range(
    &mut self,
    range_utf16: Option<Range<usize>>,
    new_text: &str,
    window: &mut Window,
    cx: &mut Context<Self>,
  ) {
    let range = self.target(range_utf16);
    self.content = format!(
      "{}{new_text}{}",
      &self.content[..range.start],
      &self.content[range.end..]
    )
    .into();
    let caret = range.start + new_text.len();
    self.selected_range = caret..caret;
    self.selection_reversed = false;
    self.marked_range = None;
    window.notify_text_input_changed(&self.focus);
    cx.notify();
  }

  fn replace_and_mark_text_in_range(
    &mut self,
    range_utf16: Option<Range<usize>>,
    new_text: &str,
    new_selected_range_utf16: Option<Range<usize>>,
    window: &mut Window,
    cx: &mut Context<Self>,
  ) {
    let range = self.target(range_utf16);
    self.content = format!(
      "{}{new_text}{}",
      &self.content[..range.start],
      &self.content[range.end..]
    )
    .into();
    self.marked_range = (!new_text.is_empty()).then(|| range.start..range.start + new_text.len());
    let start = self.offset_to_utf16(range.start);
    let inserted = new_text.encode_utf16().count();
    self.selected_range = new_selected_range_utf16
      .map(|selected| {
        self.range_from_utf16(&(start + selected.start.min(inserted)..start + selected.end.min(inserted)))
      })
      .unwrap_or_else(|| range.start + new_text.len()..range.start + new_text.len());
    self.selection_reversed = false;
    window.notify_text_input_changed(&self.focus);
    cx.notify();
  }

  fn bounds_for_range(
    &mut self,
    range_utf16: Range<usize>,
    bounds: Bounds<Pixels>,
    _: &mut Window,
    _: &mut Context<Self>,
  ) -> Option<Bounds<Pixels>> {
    let layout = self.last_layout.as_ref()?;
    if layout.text != self.content {
      return None;
    }
    let range = self.range_from_utf16(&range_utf16);
    Some(Bounds::from_corners(
      point(bounds.left() + layout.x_for_index(range.start), bounds.top()),
      point(bounds.left() + layout.x_for_index(range.end), bounds.bottom()),
    ))
  }

  fn character_index_for_point(
    &mut self,
    point: gpui::Point<Pixels>,
    _: &mut Window,
    _: &mut Context<Self>,
  ) -> Option<usize> {
    let local = self.last_bounds?.localize(&point)?;
    let layout = self.last_layout.as_ref()?;
    if layout.text != self.content {
      return None;
    }
    Some(self.offset_to_utf16(layout.closest_index_for_x(local.x)))
  }
}

/// Shapes the line, paints it with the caret or the selection, and hands the
/// platform the input handler while the field has focus.
struct TextElement {
  input: Entity<TextInput>,
}

struct Prepainted {
  line: Option<ShapedLine>,
  caret_or_selection: Option<PaintQuad>,
}

impl IntoElement for TextElement {
  type Element = Self;

  fn into_element(self) -> Self::Element {
    self
  }
}

impl Element for TextElement {
  type RequestLayoutState = ();
  type PrepaintState = Prepainted;

  fn id(&self) -> Option<ElementId> {
    None
  }

  fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
    None
  }

  fn request_layout(
    &mut self,
    _: Option<&GlobalElementId>,
    _: Option<&gpui::InspectorElementId>,
    window: &mut Window,
    cx: &mut App,
  ) -> (LayoutId, Self::RequestLayoutState) {
    let mut style = Style::default();
    style.size.width = relative(1.).into();
    style.size.height = window.line_height().into();
    (window.request_layout(style, [], cx), ())
  }

  fn prepaint(
    &mut self,
    _: Option<&GlobalElementId>,
    _: Option<&gpui::InspectorElementId>,
    bounds: Bounds<Pixels>,
    _: &mut Self::RequestLayoutState,
    window: &mut Window,
    cx: &mut App,
  ) -> Self::PrepaintState {
    let input = self.input.read(cx);
    let style = window.text_style();
    let (text, color) = if input.content.is_empty() {
      (input.placeholder.clone(), theme::ink(0.3))
    } else {
      (input.content.clone(), style.color)
    };
    let run = TextRun {
      len: text.len(),
      font: style.font(),
      color,
      background_color: None,
      underline: None,
      strikethrough: None,
    };
    let runs = match &input.marked_range {
      Some(marked) => [
        TextRun {
          len: marked.start,
          ..run.clone()
        },
        TextRun {
          len: marked.end - marked.start,
          underline: Some(UnderlineStyle {
            color: Some(run.color),
            thickness: px(1.),
            wavy: false,
          }),
          ..run.clone()
        },
        TextRun {
          len: text.len() - marked.end,
          ..run
        },
      ]
      .into_iter()
      .filter(|run| run.len > 0)
      .collect(),
      None => vec![run],
    };
    let font_size = style.font_size.to_pixels(window.rem_size());
    let line = window.text_system().shape_line(text, font_size, &runs, None);
    let selected = &input.selected_range;
    let caret_or_selection = if selected.is_empty() {
      let x = line.x_for_index(input.cursor_offset());
      fill(
        Bounds::new(
          point(bounds.left() + x, bounds.top()),
          size(px(1.5), bounds.size.height),
        ),
        theme::accent(),
      )
    } else {
      fill(
        Bounds::from_corners(
          point(bounds.left() + line.x_for_index(selected.start), bounds.top()),
          point(bounds.left() + line.x_for_index(selected.end), bounds.bottom()),
        ),
        theme::text_selection(),
      )
    };
    Prepainted {
      line: Some(line),
      caret_or_selection: Some(caret_or_selection),
    }
  }

  fn paint(
    &mut self,
    _: Option<&GlobalElementId>,
    _: Option<&gpui::InspectorElementId>,
    bounds: Bounds<Pixels>,
    _: &mut Self::RequestLayoutState,
    prepainted: &mut Self::PrepaintState,
    window: &mut Window,
    cx: &mut App,
  ) {
    let input = self.input.read(cx);
    let focus = input.focus.clone();
    let has_selection = !input.selected_range.is_empty();
    window.handle_input(&focus, ElementInputHandler::new(bounds, self.input.clone()), cx);
    let focused = focus.is_focused(window);
    // A selection shows under the text, a caret only in the focused field.
    if let Some(quad) = prepainted.caret_or_selection.take()
      && has_selection
    {
      window.paint_quad(quad);
    }
    let Some(line) = prepainted.line.take() else {
      return;
    };
    let _ = line.paint(
      bounds.origin,
      window.line_height(),
      gpui::TextAlign::Left,
      None,
      window,
      cx,
    );
    if focused
      && !has_selection
      && let Some(caret) = prepainted.caret_or_selection.take()
    {
      window.paint_quad(caret);
    }
    self.input.update(cx, |input, _| {
      input.last_layout = Some(line);
      input.last_bounds = Some(bounds);
    });
  }
}

impl Render for TextInput {
  fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let focused = self.focus.is_focused(window);
    div()
      .key_context("TextInput")
      .track_focus(&self.focus)
      .cursor(CursorStyle::IBeam)
      .on_action(cx.listener(Self::backspace))
      .on_action(cx.listener(Self::delete))
      .on_action(cx.listener(Self::left))
      .on_action(cx.listener(Self::right))
      .on_action(cx.listener(Self::home))
      .on_action(cx.listener(Self::end))
      .on_action(cx.listener(Self::select_all))
      .on_action(cx.listener(Self::paste))
      .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
      .h(px(28.))
      .w_full()
      .px(px(8.))
      .flex()
      .items_center()
      .rounded(px(6.))
      .bg(theme::hover())
      .border_1()
      .border_color(if focused { theme::accent() } else { theme::card_border() })
      .text_size(px(13.))
      .line_height(px(18.))
      .overflow_hidden()
      .child(TextElement { input: cx.entity() })
  }
}

impl Focusable for TextInput {
  fn focus_handle(&self, _: &App) -> FocusHandle {
    self.focus.clone()
  }
}
