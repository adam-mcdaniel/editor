use cursive::direction::Direction;
use cursive::event::{Event, EventResult, Key, MouseButton, MouseEvent};
use cursive::theme::{ColorStyle, Effect};
use cursive::utils::lines::simple::{prefix, simple_prefix, LinesIterator, Row};
use cursive::utils::markup::StyledString;
use cursive::view::{ScrollBase, SizeCache, View};
use cursive::Vec2;
use cursive::{Printer, With, XY};
use log::debug;
use std::cmp::min;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;


fn make_rows(text: &str, width: usize) -> Vec<Row> {
    // We can't make rows with width=0, so force at least width=1.
    let width = usize::max(width, 1);
    LinesIterator::new(text, width).show_spaces().collect()
}


pub trait Highlighter {
    fn highlight(&self, input: String) -> StyledString;
}

pub struct DefaultHighlighter;
impl Highlighter for DefaultHighlighter {
    fn highlight(&self, input: String) -> StyledString { StyledString::plain(&input) }
}


pub struct CodeArea<H> where H: Highlighter {
    highlighter: H,

    /// The lines of code in the CodeArea
    /// NO NEWLINES ARE STORED IN CONTENT
    content: Vec<StyledString>,

    /// TODO: Understand
    rows: Vec<Row>,

    /// Do we take input?
    enabled: bool,

    /// Base for scrolling features
    scrollbase: ScrollBase,

    /// Cache to avoid re-computing layout on no-op events
    size_cache: Option<XY<SizeCache>>,
    last_size: Vec2,

    /// The (row, column) of th cursor
    cursor: (usize, usize),
}

impl Default for CodeArea<DefaultHighlighter> {
    fn default() -> Self { Self::new(DefaultHighlighter) }
}


impl<H> CodeArea<H> where H: Highlighter {
    pub fn new(highlighter: H) -> Self {
        CodeArea {
            highlighter,
            content: vec![StyledString::plain("")],
            rows: Vec::new(),
            enabled: true,
            scrollbase: ScrollBase::new().right_padding(0),
            size_cache: None,
            last_size: Vec2::zero(),
            cursor: (0, 0)
        }
    }
    /// Retrieves the code from the view
    pub fn get_content(&self) -> Vec<&str> {
        self.content.iter().map(|s| s.source()).collect::<Vec<&str>>()
    }

    fn invalidate(&mut self) {
        self.size_cache = None;
    }

    /// Returns the position of the cursor in the content vector
    pub fn cursor(&self) -> (usize, usize) {
        self.cursor
    }

    /// Moves the cursor to a given position
    ///
    /// # Panics
    /// 
    /// This method will panic if the position
    /// is not valid.
    pub fn set_cursor(&mut self, cursor: (usize, usize)) {
        self.cursor = cursor;
        let focus = self.selected_row();
        self.scrollbase.scroll_to(focus);
    }

    /// Sets the content of the view
    pub fn set_content<S: Into<String>>(&mut self, content: Vec<S>) {
        let mut styled_content = vec![];
        for line in content {
            let s: String = line.into();
            assert!(!"\r\n".contains(s.chars().last().unwrap()));
            styled_content.push(self.highlighter.highlight(s));
        }
        self.content = styled_content;

        let (mut curs_row, mut curs_col) = self.cursor;
        let num_rows = self.content.len();

        if num_rows <= 0 { curs_row = 0; }
        else { curs_row = min(curs_row, num_rows-1); }

        // Panic if the cursor is not valid
        // THIS SHOULD NEVER PANIC!!
        assert!(curs_row > 0 && curs_row <= num_rows-1);

        curs_col = self.content[curs_row].source().len() - 1;
        

        if let Some(size) = self.size_cache.map(|s| s.map(|s| s.value)) {
            self.invalidate();
            self.compute_rows(size);
        }
    }

    /// Disable this view
    ///
    /// A disabled view cannot be selected.
    pub fn disable(&mut self) {
        self.enabled = false;
    }


    /// Re-enables this view
    pub fn enable(&mut self) {
        self.enabled = true;
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    fn selected_row(&self) -> usize {
        let (row, _) = self.cursor;
        row
    }

    fn selected_col(&self) -> usize {
        let (_, col) = self.cursor;
        col
    }

    fn page_up(&mut self) {
        for _ in 0..5 {
            self.move_up();
        }
    }

    fn page_down(&mut self) {
        for _ in 0..5 {
            self.move_down();
        }
    }

    fn move_up(&mut self) {
        let (row, col) = self.cursor;
        let contents = self.get_content();

        // If the row is already zero, we cant move up
        if row == 0 { return; }

        let new_row  = row-1;
        let new_col = contents[new_row].len() - 1;
        self.cursor = (new_row, new_col);
    }


    fn move_down(&mut self) {
        let (row, col) = self.cursor;
        let contents = self.get_content();

        // If the row is already zero, we cant move up
        if row == contents.len() { return; }

        let new_row  = row+1;
        let new_col = contents[new_row].len() - 1;
        self.cursor = (new_row, new_col);
    }

    fn move_left(&mut self) {
        let (row, col) = self.cursor;

        let new_col;
        if col == 0 {
            self.move_up();
            new_col = self.get_content()[self.selected_row()].len();
        } else {
            new_col = col - 1;
        }

        self.cursor = (self.selected_row(), new_col);
    }

    fn move_right(&mut self) { 
        let (row, col) = self.cursor;

        let new_col;
        if col + 1 == self.get_content()[self.selected_row()].len() {
            self.move_down();
            new_col = 0;
        } else {
            new_col = col + 1;
        }

        self.cursor = (self.selected_row(), new_col);
    }

    fn is_cache_valid(&self, size: Vec2) -> bool {
        match self.size_cache {
            None => false,
            Some(ref last) => last.x.accept(size.x) && last.y.accept(size.y),
        }
    }

    fn fix_ghost_row(&mut self) {
        if self.rows.is_empty()
            || self.rows.last().unwrap().end != self.content.len()
        {
            // Add a fake, empty row at the end.
            self.rows.push(Row {
                start: self.content.len(),
                end: self.content.len(),
                width: 0,
            });
        }
    }


    fn soft_compute_rows(&mut self, size: Vec2) {
        if self.is_cache_valid(size) {
            debug!("Cache is still valid.");
            return;
        }
        debug!("Soft computing cache");

        let mut available = size.x;


        self.rows = make_rows(&self.content, available);
        self.fix_ghost_row();

        if self.rows.len() > size.y {
            available.saturating_sub(1);
            self.rows = make_rows(&self.content, available);
            self.fix_ghost_row();
        }

        if !self.rows.is_empty() {
            self.size_cache = Some(SizeCache::build(size, size));
        }
    }

    fn compute_rows(&mut self, size: Vec2) {
        self.soft_compute_rows(size);
        self.scrollbase.set_heights(size.y, self.rows.len());
    }

    fn backspace(&mut self) {
        self.move_left();
        self.delete();
    }

    fn delete(&mut self) {
        if self.cursor == self.content.len() {
            return;
        }
        let (row, col) = self.cursor;
        let row_str = self.content[row];
        
        if col+1 == row_str.len() {
            self.content.remove(row+1);
            let next_line = self.content[row+1];
            self.content[row].append(self.highlighter.highlight(next_line.into()));
        } else {
            self.content[row-1].append(self.highlighter.highlight(row_str));
        }
    }
}
