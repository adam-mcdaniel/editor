use cursive::direction::Direction;
use cursive::event::{Callback, Event, EventResult, Key, MouseButton, MouseEvent};
use cursive::theme::{BaseColor, Color, ColorStyle, ColorType, Effect, Style};
use cursive::utils::lines::simple::{prefix, simple_prefix, LinesIterator, Row};
use cursive::utils::markup::StyledString;
use cursive::view::{ScrollBase, SizeCache, View};
use cursive::Rect;
use cursive::Vec2;
use cursive::{Printer, With, XY};
use log::debug;
use std::cmp::{max, min};
use std::fs::{read_to_string, write};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// Maximum space that the line number prefix will consume
/// This includes the space and `|` character after the number
pub const LN_SPACE: i32 = 6;

/// An object used to highlight displayed text
pub trait Highlighter: Default + 'static {
    fn highlight(&self, code: impl ToString) -> StyledString;
}

#[derive(Default)]
pub struct DefaultHighlighter;
impl Highlighter for DefaultHighlighter {
    fn highlight(&self, code: impl ToString) -> StyledString {
        let code = code.to_string() + " ";
        let mut result = StyledString::plain("");
        let mut in_string = false;

        let mut string_color = ColorStyle::secondary();
        string_color.back = ColorType::Color(Color::Light(BaseColor::Green));

        let mut number_color = ColorStyle::secondary();
        number_color.back = ColorType::Color(Color::Light(BaseColor::Yellow));

        let mut keyword_color = ColorStyle::secondary();
        keyword_color.back = ColorType::Color(Color::Light(BaseColor::Magenta));

        let mut symbol_color = ColorStyle::secondary();
        symbol_color.back = ColorType::Color(Color::Dark(BaseColor::Yellow));

        let mut type_color = ColorStyle::secondary();
        type_color.back = ColorType::Color(Color::Dark(BaseColor::Blue));

        let types = vec![
            "Self", "Vec", "i32", "i64", "f32", "f64", "int", "double", "float", "char", "bool",
            "self", "String", "str", "true", "false", "True", "False",
        ];

        let keywords = vec![
            "class", "struct", "use", "import", "trait", "type", "impl", "pub", "let", "if",
            "while", "for", "else", "mut", "in", "match", "continue", "break", "fn", "def",
            "lambda", "return", "new", "data", "begin", "end", "then", "is", "enum", "do",
            "var", "static", "public", "private", "where", "include", "define", "pragma",
            "const"
        ];

        let symbols = [';', ',', ':', '?', '{', '}', '(', ')', '!'];

        let mut skip = 0;

        for (i, ch) in code.chars().enumerate() {
            for key in &keywords {
                if code.len() < i + key.len() + 1 {
                    continue;
                }

                if i > 0
                    && code
                        .chars()
                        .nth(i - 1)
                        .and_then(|ch| Some(ch.is_alphabetic()))
                        .unwrap_or(false)
                {
                    continue;
                }

                if &code[i..i + key.len()] == *key
                    && code
                        .chars()
                        .nth(i + key.len())
                        .and_then(|ch| Some(!ch.is_alphabetic()))
                        .unwrap_or(false)
                {
                    result.append_styled(*key, Style::from(keyword_color.clone()));
                    skip = key.len();
                    break;
                }
            }

            for t in &types {
                if code.len() < i + t.len() + 1 {
                    continue;
                }

                if i > 0
                    && code
                        .chars()
                        .nth(i - 1)
                        .and_then(|ch| Some(ch.is_alphabetic()))
                        .unwrap_or(false)
                {
                    continue;
                }

                if &code[i..i + t.len()] == *t
                    && code
                        .chars()
                        .nth(i + t.len())
                        .and_then(|ch| Some(!ch.is_alphabetic()))
                        .unwrap_or(false)
                {
                    result.append_styled(*t, Style::from(type_color.clone()));
                    skip = t.len();
                    break;
                }
            }

            if skip > 0 {
                skip -= 1;
                continue;
            }

            match ch {
                '\"' if i > 1 && code.chars().nth(max(i - 1, 0) as usize) == Some('\\') => {
                    result.append_styled("\"", Style::from(string_color.clone()));
                }
                '\"' => {
                    result.append_styled("\"", Style::from(string_color.clone()));
                    in_string = !in_string;
                }
                ch if ch.is_digit(10) => {
                    result.append_styled(&ch.to_string(), Style::from(number_color.clone()))
                }
                ch if in_string => {
                    result.append_styled(&ch.to_string(), Style::from(string_color.clone()))
                }
                ch if symbols.contains(&ch) => {
                    result.append_styled(&ch.to_string(), Style::from(symbol_color.clone()))
                }
                ch => result.append_plain(&ch.to_string()),
            }
        }
        result
    }
}

/// Multi-lines text editor.
///
/// A `TextArea` will attempt to grow vertically and horizontally
/// dependent on the content.  Wrap it in a `ResizedView` to
/// constrain its size.
///
/// # Examples
///
/// ```
/// use cursive::traits::{Resizable, Identifiable};
/// use cursive::views::TextArea;
///
/// let text_area = TextArea::new()
///     .content("Write description here...")
///     .with_name("text_area")
///     .fixed_width(30)
///     .min_height(5);
/// ```
pub struct CodeArea<H>
where
    H: Highlighter,
{
    /// Filename for saving
    filename: String,

    /// The highlighter for displaying code syntax
    highlighter: H,

    /// The marker used for selection
    selection_marker: Option<(i32, i32)>,

    /// The string to comment out code
    comment_prefix: String,

    /// Stores the content of the code area
    contents: Vec<String>,

    /// Stores cut and copied text
    clipboard: String,

    /// When `false`, we don't take any input.
    enabled: bool,

    /// Base for scrolling features
    scrollbase: ScrollBase,

    /// Byte offset of the currently selected grapheme.
    cursor: (i32, i32),
}

impl<H> Default for CodeArea<H>
where
    H: Highlighter,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<H> CodeArea<H>
where
    H: Highlighter,
{
    pub fn new() -> Self {
        Self {
            highlighter: H::default(),
            filename: String::new(),
            selection_marker: None,
            comment_prefix: String::from("// "),
            clipboard: String::new(),
            contents: vec![String::new(), String::new()],
            enabled: true,
            scrollbase: ScrollBase::new().right_padding(0),
            cursor: (0, 0),
        }
    }

    pub fn open_file(mut self, file: impl ToString) -> Self {
        let result = read_to_string(file.to_string());
        self.filename = file.to_string();
        if let Ok(contents) = result {
            self.with_content(contents)
        } else {
            self
        }
    }

    pub fn with_content(mut self, content: impl ToString) -> Self {
        self.insert_str(content);
        self.cursor = (0, 0);
        self
    }

    pub fn with_comment(mut self, comment: impl ToString) -> Self {
        self.comment_prefix = comment.to_string();
        self
    }

    pub fn save_content(&mut self) {
        write(&self.filename, self.contents.join("\n"));
    }

    pub fn is_selecting(&self) -> bool {
        self.selection_marker.is_some()
    }

    pub fn continue_selection(&mut self) {
        if !self.is_selecting() {
            self.selection_marker = Some(self.cursor)
        }
    }

    pub fn forget_selection(&mut self) {
        self.selection_marker = None
    }

    pub fn row(&mut self, i: i32) -> &mut String {
        let len = (self.contents.len() - 1) as i32;
        &mut self.contents[min(max(i, 0), len) as usize]
    }

    pub fn row_len(&self, i: i32) -> i32 {
        self.contents[min(max(i, 0), (self.contents.len() - 1) as i32) as usize].len() as i32
    }

    /// Cuts the current line of the cursor
    pub fn cut(&mut self) {
        self.fix();

        let (row, col) = self.cursor;
        // Will be stored into clipboard
        let mut result = String::new();

        if let Some((mrow, mcol)) = self.selection_marker {
            if (mrow, mcol) == (row, col) {
                return;
            }

            let top;
            let bottom;
            if mrow < row {
                top = (mrow, mcol);
                bottom = (row, col);
            } else if row < mrow {
                top = (row, col);
                bottom = (mrow, mcol);
            } else if row == mrow && mcol < col {
                top = (mrow, mcol);
                bottom = (row, col);
            } else if row == mrow && col < mcol {
                top = (row, col);
                bottom = (mrow, mcol);
            } else {
                unreachable!()
            }

            let mut chars_between = 0;
            let (top_row, top_col) = top;
            let (bottom_row, bottom_col) = bottom;
            for ln in top_row..bottom_row + 1 {
                if ln == top_row && ln == bottom_row {
                    chars_between += bottom_col - top_col - 1;
                } else if ln == top_row {
                    chars_between += self.row_len(ln) - top_col;
                } else if top_row < ln && ln < bottom_row {
                    chars_between += self.row_len(ln) + 1;
                } else if ln == bottom_row {
                    chars_between += bottom_col;
                }
            }

            self.cursor = top;

            for _ in 0..chars_between + 1 {
                if let Some(ch) = self.row(top_row).chars().nth(top_col as usize) {
                    result.push(ch);
                } else {
                    result.push('\n');
                }
                self.delete();
            }

            self.clipboard = result;
        } else {
            if self.contents.len() > 1 {
                let result = self.row(row).clone() + "\n";
                self.contents.remove(row as usize);
                self.clipboard = result;
                self.move_cursor_home();
            }
        }

        self.fix();
    }

    pub fn copy(&mut self) {
        let save_pos = self.cursor;
        if self.is_selecting() {
            self.cut();
            self.paste();
            self.cursor = save_pos;
        } else {
            self.clipboard = String::from("\n") + self.row(save_pos.0);
        }

        self.fix();
    }

    pub fn paste(&mut self) {
        let content = self.clipboard.clone();
        self.insert_str(&content);
        self.fix();
    }

    pub fn copy_line_down(&mut self) {
        let (row, _) = self.cursor;
        let current_line = self.row(row).clone();
        self.contents.insert(row as usize, current_line);
        self.move_cursor_down();
        self.fix();
    }

    /// Comments out the current line of the cursor if the line is not
    /// already commented. If the line is commented, this will uncomment
    /// the line.
    pub fn comment_current_line(&mut self) {
        let (row, col) = self.cursor;
        let len = self.comment_prefix.len();
        let comment = self.comment_prefix.clone();

        self.cursor = (row, 0);

        let should_do_comment;
        if self.row(row).len() < len {
            // Do the comment because its not possible to already
            // have a comment but have a line shorter than a comment
            should_do_comment = true;
        } else if &self.row(row)[0..len] == comment {
            // Uncomment because its already commented
            should_do_comment = false;
        } else {
            should_do_comment = true;
        }

        if should_do_comment {
            self.insert_str(comment);
            self.cursor = (row, col + len as i32);
        } else {
            for _ in 0..len {
                self.row(row).remove(0);
            }

            if col <= len as i32 {
                self.cursor = (row, 0);
            } else {
                self.cursor = (row, col - len as i32);
            }
        }

        self.fix();
    }

    /// Comments out the selected lines of the cursor (if lines have been selected)
    pub fn comment_selection(&mut self) {
        let (init_row, init_col) = self.cursor;
        if let Some((marker_row, _)) = self.selection_marker {
            let begin_row = min(marker_row, init_row);
            let end_row = max(marker_row, init_row);

            for row in begin_row..end_row + 1 {
                self.cursor = (row, 0);
                self.comment_current_line();
            }
            self.cursor = (init_row, init_col + self.comment_prefix.len() as i32);
        }

        self.fix();
    }

    pub fn move_line_up(&mut self) {
        let (row, col) = self.cursor;
        let current_line = self.row(row).clone();
        let previous_line = self.row(row - 1).clone();

        *self.row(row) = previous_line;
        *self.row(row - 1) = current_line;
        self.cursor = (max(row - 1, 0), col);
    }

    pub fn move_line_down(&mut self) {
        let (row, col) = self.cursor;
        let current_line = self.row(row).clone();
        let next_line = self.row(row + 1).clone();

        *self.row(row) = next_line;
        *self.row(row + 1) = current_line;
        self.cursor = (min(row + 1, (self.contents.len() - 1) as i32), col);
    }

    pub fn move_cursor_home(&mut self) {
        let (row, _) = self.cursor;
        self.cursor = (row, 0);
    }

    pub fn move_cursor_end(&mut self) {
        let (row, _) = self.cursor;
        self.cursor = (row, self.row_len(row));
    }

    /// Move the cursor left one character
    pub fn move_cursor_left(&mut self) {
        match self.cursor {
            // You cant move left!
            (0, 0) => return,
            // (row, 0) => self.cursor = (row-1, 0),
            (row, 0) => self.cursor = (row - 1, self.row_len(row - 1)),
            (row, col) => self.cursor = (row, col - 1),
        }

        debug!("cursor {} {}", self.cursor.0, self.cursor.1);

        self.fix();
    }

    /// Move the cursor right one character
    pub fn move_cursor_right(&mut self) {
        match self.cursor {
            (row, _) if self.row_len(row) == 0 => self.cursor = (row + 1, 0),
            (row, col) if self.row_len(row) == col => self.cursor = (row + 1, 0),
            (row, col) => self.cursor = (row, col + 1),
        }

        self.fix();
    }

    /// Move the cursor down one character
    pub fn move_cursor_up(&mut self) {
        match self.cursor {
            // You cant move up!
            (0, _) => return,
            (row, col) => self.cursor = (row - 1, col),
        }

        self.fix();
    }

    /// Move the cursor down one character
    pub fn move_cursor_down(&mut self) {
        let (row, col) = self.cursor;
        self.cursor = (row + 1, col);
        self.fix();
    }

    /// Move cursor a page up
    pub fn move_page_up(&mut self) {
        for _ in 0..8 {
            self.move_cursor_up();
        }
    }

    /// Move cursor a page down
    pub fn move_page_down(&mut self) {
        for _ in 0..8 {
            self.move_cursor_down();
        }
    }

    /// Delete a character at the cursor
    pub fn delete(&mut self) {
        self.fix();

        let (row, col) = self.cursor;

        match (row, col) {
            (row, col) if col >= self.row_len(row) && row < (self.contents.len() - 1) as i32 => {
                let s = self.row(row + 1).clone();
                *self.row(row) += &s;
                self.contents.remove((row + 1) as usize);
            }
            (row, col) if row < (self.contents.len() - 1) as i32 => {
                self.row(row).remove(col as usize);
            }
            _ => {}
        }

        self.fix();
    }

    /// Move left and delete
    pub fn backspace(&mut self) {
        if self.cursor == (0, 0) {
            return;
        }
        self.move_cursor_left();
        self.delete();
    }

    /// Insert a character at the cursor
    pub fn insert(&mut self, ch: char) {
        let (row, col) = self.cursor;
        match ch {
            '\n' => {
                let before_cursor = String::from(&self.row(row)[..col as usize]);
                let after_cursor = String::from(&self.row(row)[col as usize..]);

                *self.row(row) = before_cursor;
                self.contents.insert((row + 1) as usize, after_cursor);
                self.cursor = (row + 1, 0);
            }
            '\t' => self.insert_str("    "),
            other => {
                self.row(row).insert(col as usize, other);
                self.move_cursor_right();
            }
        }
        self.fix();
    }

    /// Insert a string at the cursor
    pub fn insert_str(&mut self, s: impl ToString) {
        for ch in s.to_string().chars() {
            self.insert(ch);
        }

        self.fix();
    }

    /// Fix the cursor if its invalid
    pub fn fix_cursor(&mut self) {
        let (mut row, mut col) = self.cursor;
        // Check if the cursor is greater than the number of rows
        if row >= self.contents.len() as i32 {
            row = max((self.contents.len() - 1) as i32, 0);
            col = self.row_len(row)
        }
        if col > self.row_len(row) {
            col = self.row_len(row)
        }

        self.cursor = (row, col);
    }

    /// Check to see if there are any newlines in the content.
    /// Also, confirm there is an extra line at the end of the file.
    pub fn fix_newline(&mut self) {
        // Get rid of any newlines (there shouldnt be any)
        for line in &mut self.contents {
            *line = line.replace("\n", "");
        }

        // If theres no empty line, add one!
        if Some(&String::from("")) != self.contents.last() {
            self.contents.push(String::from(""));
        }
    }

    /// This method attempts to fix problems with the editor
    /// such as invalid cursor position, no empty newline at end of file, etc.
    pub fn fix(&mut self) {
        self.fix_cursor();
        self.fix_newline();
    }
}

impl<H> View for CodeArea<H>
where
    H: Highlighter,
{
    fn required_size(&mut self, constraint: Vec2) -> Vec2 {
        let scroll_width = if self.contents.len() > constraint.y {
            1
        } else {
            0
        };
        // Vec2::new(
        //     scroll_width + 1 + self.contents.iter().map(|r| r.len()).max().unwrap_or(1),
        //     self.contents.len()
        // )
        Vec2::new(
            max(
                100,
                scroll_width + 1 + self.contents.iter().map(|r| r.len()).max().unwrap_or(1),
            ),
            max(self.contents.len(), 256),
        )
    }

    fn draw(&self, printer: &Printer<'_, '_>) {
        printer.with_color(ColorStyle::secondary(), |printer| {
            let effect = if self.enabled && printer.enabled {
                Effect::Reverse
            } else {
                Effect::Simple
            };

            let w = if self.scrollbase.scrollable() {
                printer.size.x.saturating_sub(1)
            } else {
                printer.size.x
            };
            printer.with_effect(effect, |printer| {
                for y in 0..printer.size.y {
                    printer.print_hline((0, y), w + LN_SPACE as usize, " ");
                }
            });

            self.scrollbase.draw(printer, |printer, i| {
                let text = &self.contents[i];

                let (row, col) = self.cursor;
                printer.with_effect(effect, |printer| {
                    printer.print_styled((LN_SPACE, 0), (&self.highlighter.highlight(&text)).into());
                });
                if printer.focused && i as i32 == row {
                    printer.print_styled((col + LN_SPACE, 0), (&StyledString::from("_")).into());
                }
                if let Some((mrow, mcol)) = self.selection_marker {
                    if printer.focused && i as i32 == mrow {
                        printer.print_styled((mcol + LN_SPACE, 0), (&StyledString::from("_")).into());
                    }
                }

                printer.with_effect(effect, |printer| {
                    printer
                        .print_styled((0, 0), (&StyledString::from(format!("{:<4}| ", i+1))).into());
                });
            });
        });
    }

    fn on_event(&mut self, event: Event) -> EventResult {
        self.fix();
        let mut fix_scroll = true;
        let mut is_shifting = false;
        let mut quit = false;
        match event {
            // Event::CtrlChar('k') => self.cut_line(),
            Event::CtrlChar('q') => quit = true,
            Event::CtrlChar('s') => self.save_content(),
            Event::CtrlChar('v') => self.paste(),
            Event::CtrlChar('f') => self.copy(),
            Event::CtrlChar('x') => self.cut(),
            Event::CtrlChar('k') => {
                if self.is_selecting() {
                    self.comment_selection()
                } else {
                    self.comment_current_line()
                }
                is_shifting = true;
            }
            Event::CtrlChar('d') => self.copy_line_down(),
            Event::Char(ch) => self.insert(ch),
            Event::Key(Key::Enter) => self.insert('\n'),
            Event::Key(Key::Del) => self.delete(),
            Event::Key(Key::Backspace) => self.backspace(),
            Event::Key(Key::Tab) => self.insert('\t'),
            Event::Key(Key::Enter) => self.insert('\n'),
            Event::Key(Key::Del) => self.delete(),
            Event::Key(Key::Backspace) => self.backspace(),

            Event::Key(Key::Home) => self.move_cursor_home(),
            Event::Key(Key::End) => self.move_cursor_end(),
            Event::Key(Key::PageUp) => self.move_page_up(),
            Event::Shift(Key::PageUp) => {
                self.continue_selection();
                self.move_page_up();
                is_shifting = true;
            }
            Event::Key(Key::PageDown) => self.move_page_down(),
            Event::Shift(Key::PageDown) => {
                self.continue_selection();
                self.move_page_down();
                is_shifting = true;
            }
            Event::Ctrl(Key::Up) => self.move_line_up(),
            Event::Key(Key::Up) => self.move_cursor_up(),
            Event::Shift(Key::Up) => {
                self.continue_selection();
                self.move_cursor_up();
                is_shifting = true;
            }
            Event::Ctrl(Key::Down) => self.move_line_down(),
            Event::Key(Key::Down) => self.move_cursor_down(),
            Event::Shift(Key::Down) => {
                self.continue_selection();
                self.move_cursor_down();
                is_shifting = true;
            }
            Event::Key(Key::Left) => self.move_cursor_left(),
            Event::Shift(Key::Left) => {
                self.continue_selection();
                self.move_cursor_left();
                is_shifting = true;
            }
            Event::Key(Key::Right) => self.move_cursor_right(),
            Event::Shift(Key::Right) => {
                self.continue_selection();
                self.move_cursor_right();
                is_shifting = true;
            }
            Event::Mouse {
                event: MouseEvent::WheelUp,
                ..
            } if self.scrollbase.can_scroll_up() => {
                fix_scroll = false;
                self.scrollbase.scroll_up(5);
            }
            Event::Mouse {
                event: MouseEvent::WheelDown,
                ..
            } if self.scrollbase.can_scroll_down() => {
                fix_scroll = false;
                self.scrollbase.scroll_down(5);
            }
            Event::Mouse {
                event: MouseEvent::Hold(MouseButton::Left),
                position,
                offset,
            } => {
                fix_scroll = false;
                let position = position.saturating_sub(offset);
                self.scrollbase.drag(position);
            }
            _ => return EventResult::Ignored,
        }

        if !is_shifting {
            self.forget_selection()
        }

        if fix_scroll {
            let focus = self.cursor.0;
            self.scrollbase.scroll_to(focus as usize);
        }

        if quit {
            EventResult::Consumed(Some(Callback::from_fn_mut(|s| s.quit())))
        } else {
            EventResult::Consumed(None)
        }
    }

    fn take_focus(&mut self, _: Direction) -> bool {
        self.enabled
    }

    fn layout(&mut self, size: Vec2) {
        self.scrollbase.set_heights(size.y, self.contents.len());
    }

    fn important_area(&self, _: Vec2) -> Rect {
        // The important area is a single character
        let (row, col) = self.cursor;
        Rect::from_size((col, row), (1, 1))
    }
}
