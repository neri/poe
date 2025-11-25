//! TUI text buffer implementation.

use crate::fixed_str::FixedStrBuf;
use crate::prelude::*;
use alloc::boxed::Box;
use alloc::vec::Vec;
use core::cell::UnsafeCell;
use core::num::NonZero;

/// Window Buffer for Text User Interface.
pub struct TuiWindowBuffer<TCHAR: TChar> {
    buffer: UnsafeCell<TextBuffer<TCHAR>>,
    origin: Point,
    pub insets: Inset,
    redraw_region: Diagonal,
    pub default_attr: TuiAttribute,
}

/// Type alias for a TUI window buffer with Ascii characters.
pub type TuiWindowBufferAscii = TuiWindowBuffer<u8>;

/// Type alias for a TUI window buffer with Unicode characters.
pub type TuiWindowBufferUcs = TuiWindowBuffer<char>;

/// Text Buffer for Text User Interface.
pub struct TextBuffer<TCHAR: TChar> {
    pub size: Size,
    pub text_buffer: UnsafeCell<Box<[TCHAR]>>,
    pub attr_buffer: UnsafeCell<Box<[TuiAttribute]>>,
}

/// A view of Text Buffer.
pub struct TextBufferView<'a, TCHAR: TChar> {
    pub buffer: &'a TextBuffer<TCHAR>,
    pub frame: Rect,
}

/// Trait for drawing operations on a text buffer.
pub trait TextBufferDrawing<TCHAR: TChar> {
    /// Put a character at the specified position.
    fn put_char_at(&mut self, pos: Point, ch: TCHAR, attr: TuiAttribute) -> Option<()>;

    /// Get the character and attribute at the specified position.
    fn get_char_at(&self, pos: Point) -> Option<(TCHAR, TuiAttribute)>;

    /// Put a string at the specified origin.
    fn put_string_at(&mut self, origin: Point, s: &str, attr: TuiAttribute) {
        let mut pos = origin;
        for ch in s.chars() {
            match ch {
                '\0'..='\x1f' => {
                    // Ignore control characters
                }
                _ => {
                    if self.put_char_at(pos, TCHAR::from_char(ch), attr).is_none() {
                        break;
                    }
                    pos.x += 1;
                }
            }
        }
    }

    /// Draw a line from start to end using Bresenham's line algorithm.
    fn draw_line(&mut self, start: Point, end: Point, ch: TCHAR, attr: TuiAttribute) {
        let _ = start.line_to(end, |p| {
            self.put_char_at(p, ch, attr);
        });
    }

    /// Draw a vertical line starting from origin with the specified length.
    fn draw_vline(&mut self, origin: Point, length: i32, ch: TCHAR, attr: TuiAttribute) {
        let end = Point::new(origin.x, origin.y + length - 1);
        self.draw_line(origin, end, ch, attr);
    }

    /// Draw a horizontal line starting from origin with the specified length.
    fn draw_hline(&mut self, origin: Point, length: i32, ch: TCHAR, attr: TuiAttribute) {
        let end = Point::new(origin.x + length - 1, origin.y);
        self.draw_line(origin, end, ch, attr);
    }

    /// Draw a rectangle defined by the specified rect.
    fn draw_rect(&mut self, rect: Rect, ch: TCHAR, attr: TuiAttribute) {
        let top_left = rect.top_left();
        let bottom_right = rect.bottom_right().unwrap_or(top_left);
        let top_right = Point::new(bottom_right.x, top_left.y);
        let bottom_left = Point::new(top_left.x, bottom_right.y);

        self.draw_hline(top_left, rect.size().width, ch, attr);
        self.draw_hline(bottom_left, rect.size().width, ch, attr);
        self.draw_vline(top_left, rect.size().height, ch, attr);
        self.draw_vline(top_right, rect.size().height, ch, attr);
    }

    /// Fill a rectangle defined by the specified rect.
    fn fill_rect(&mut self, rect: Rect, ch: TCHAR, attr: TuiAttribute) {
        let top_left = rect.top_left();
        let right_bottom = rect.bottom_right().unwrap_or(top_left);
        for y in top_left.y..=right_bottom.y {
            self.draw_hline(Point::new(top_left.x, y), rect.size().width, ch, attr);
        }
    }
}

impl<TCHAR: TChar> TuiWindowBuffer<TCHAR> {
    #[inline]
    pub fn new(
        frame: Rect,
        insets: Inset,
        default_attr: TuiAttribute,
        // title: Option<&str>,
    ) -> Self {
        Self {
            buffer: UnsafeCell::new(TextBuffer::new(frame.size(), default_attr)),
            origin: frame.top_left(),
            insets,
            default_attr,
            redraw_region: Diagonal::INVALID,
            // title: title.map(|s| s.to_string()),
        }
    }

    #[inline]
    pub fn frame(&self) -> Rect {
        Rect::new(self.origin, self.buffer().size)
    }

    #[inline]
    pub fn bounds(&self) -> Rect {
        Rect::new(Point::zero(), self.buffer().size)
    }

    #[inline]
    pub fn client_rect(&self) -> Option<Rect> {
        self.bounds().insets(&self.insets)
    }

    pub fn invalidate_rect(&mut self, region: Option<&Rect>) {
        if let Some(region) = region {
            let mut region = *region;
            region.clip(&self.bounds());
            self.redraw_region.expand_rect(&region);
        } else {
            self.redraw_region.expand_rect(&self.bounds());
        }
    }

    /// Draw a simple title bar at the top of the buffer.
    pub fn draw_simple_title(&mut self, s: &str, back_attr: TuiAttribute, text_attr: TuiAttribute) {
        self.draw_hline(
            Point::zero(),
            self.buffer().size.width,
            TCHAR::from_char(' '),
            back_attr,
        );
        let left = ((self.buffer().size.width as i32 - s.len() as i32) / 2).max(1);
        self.buffer_mut()
            .put_string_at(Point::new(left, 0), s, text_attr);
    }

    /// Put a string at the specified origin.
    ///
    /// Text will wrap to the next line on newline characters or when reaching the end of the line.
    ///
    /// # Returns
    ///
    /// Returns the position after the last character put, or `None` if the string could not be fully put.
    pub fn put_text(
        &mut self,
        origin: Point,
        s: &str,
        attr: TuiAttribute,
        max_lines: usize,
    ) -> Option<Point> {
        let mut max_lines = NonZero::new(max_lines)
            .unwrap_or(NonZero::<usize>::MAX)
            .get();
        let Some(bounds) = self.client_rect().and_then(|v| v.to_diagonal()) else {
            return None;
        };
        let mut pos = origin;
        if pos.x < bounds.top_left.x || pos.y < bounds.top_left.y {
            return None;
        }
        for ch in s.chars() {
            match ch {
                '\n' => {
                    pos.x = bounds.top_left.x;
                    pos.y += 1;
                    if max_lines > 0 {
                        max_lines -= 1;
                    } else {
                        return None;
                    }
                }
                '\0'..'\x1f' => {
                    // Ignore control characters
                }
                _ => {
                    self.put_char_at(pos, TCHAR::from_char(ch), attr);
                    pos.x += 1;
                }
            }
            if pos.x > bounds.bottom_right.x {
                pos.x = bounds.top_left.x;
                pos.y += 1;
                if max_lines > 0 {
                    max_lines -= 1;
                } else {
                    return None;
                }
            }
            if pos.y > bounds.bottom_right.y {
                return None;
            }
        }
        Some(pos)
    }

    #[inline]
    fn buffer<'a>(&'a self) -> &'a TextBuffer<TCHAR> {
        // SAFETY: I believe that each element of the array is safe to access simultaneously.
        unsafe { &*self.buffer.get() }
    }

    #[inline]
    fn buffer_mut<'a>(&'a mut self) -> &'a mut TextBuffer<TCHAR> {
        self.buffer.get_mut()
    }

    #[inline]
    pub fn view<'a>(&'a self) -> TextBufferView<'a, TCHAR> {
        self.buffer().view()
    }

    #[inline]
    pub fn sub_view<'a>(&'a self, region: Rect) -> Option<TextBufferView<'a, TCHAR>> {
        self.buffer().sub_view(region)
    }

    #[inline]
    pub fn client_area_view<'a>(&'a self) -> Option<TextBufferView<'a, TCHAR>> {
        let client_rect = self.client_rect()?;
        self.sub_view(client_rect)
    }

    /// Perform redraw if needed.
    ///
    /// # Returns
    ///
    /// Returns `Some(())` if redraw was performed, `None` otherwise.
    pub fn redraw_if_needed<T: TuiDrawTarget + ?Sized>(&mut self, target: &mut T) -> Option<()> {
        if let Some(redraw_region) = self.redraw_region.to_rect() {
            self.draw_subregion_to(target, redraw_region);
            self.redraw_region = Diagonal::INVALID;
            Some(())
        } else {
            None
        }
    }

    /// Draw the entire buffer to the specified draw target.
    pub fn draw_to<T: TuiDrawTarget + ?Sized>(&self, target: &mut T) {
        self.draw_subregion_to(target, Rect::new(Point::new(0, 0), self.buffer().size));
    }

    /// Draw a subregion of the buffer to the specified draw target.
    pub fn draw_subregion_to<T: TuiDrawTarget + ?Sized>(&self, target: &mut T, region: Rect) {
        let mut region = region;
        if region.clip(&self.bounds()).is_none() {
            return;
        };
        let top_left = region.top_left();
        let Some(bottom_right) = region.bottom_right() else {
            return;
        };
        let mut buf = FixedStrBuf::<256>::new();
        for y in top_left.y..=bottom_right.y {
            let mut last_attr = TuiAttribute::default();
            let mut last_pos = Point::default();
            buf.clear();
            for x in top_left.x..=bottom_right.x {
                let Some((ch, attr)) = self.get_char_at(Point::new(x, y)) else {
                    return;
                };
                if buf.is_empty() {
                    last_attr = attr;
                    last_pos = Point::new(x, y);
                    let _ = buf.push(ch.into_char());
                } else {
                    if last_attr == attr {
                        let _ = buf.push(ch.into_char());
                    } else {
                        target.draw(last_pos + self.origin, buf.as_str(), last_attr);
                        buf.clear();
                        last_attr = attr;
                        last_pos = Point::new(x, y);
                        let _ = buf.push(ch.into_char());
                    }
                }
            }
            if !buf.is_empty() {
                target.draw(last_pos + self.origin, buf.as_str(), last_attr);
            }
        }
    }
}

impl<TCHAR: TChar> TextBufferDrawing<TCHAR> for TuiWindowBuffer<TCHAR> {
    #[inline]
    fn put_char_at(&mut self, pos: Point, ch: TCHAR, attr: TuiAttribute) -> Option<()> {
        self.buffer_mut().put_char_at(pos, ch, attr).map(|_| {
            self.redraw_region.expand_point(pos);
        })
    }

    #[inline]
    fn get_char_at(&self, pos: Point) -> Option<(TCHAR, TuiAttribute)> {
        self.buffer().get_char_at(pos)
    }

    fn draw_hline(&mut self, origin: Point, length: i32, ch: TCHAR, attr: TuiAttribute) {
        self.redraw_region
            .expand_rect(&Rect::new(origin, Size::new(length as i32, 1)));
        self.buffer_mut().draw_hline(origin, length, ch, attr);
    }

    fn draw_vline(&mut self, origin: Point, length: i32, ch: TCHAR, attr: TuiAttribute) {
        self.redraw_region
            .expand_rect(&Rect::new(origin, Size::new(1, length as i32)));
        self.buffer_mut().draw_vline(origin, length, ch, attr);
    }

    fn draw_rect(&mut self, rect: Rect, ch: TCHAR, attr: TuiAttribute) {
        self.redraw_region.expand_rect(&rect);
        self.buffer_mut().draw_rect(rect, ch, attr);
    }

    fn fill_rect(&mut self, rect: Rect, ch: TCHAR, attr: TuiAttribute) {
        self.redraw_region.expand_rect(&rect);
        self.buffer_mut().fill_rect(rect, ch, attr);
    }
}

impl<TCHAR: TChar> TextBuffer<TCHAR> {
    #[inline]
    pub fn new(size: Size, default_attr: TuiAttribute) -> Self {
        let buf_size = size.width as usize * size.height as usize;
        let mut text_buffer = Vec::with_capacity(buf_size);
        text_buffer.resize(buf_size, TCHAR::from_char(' '));
        let mut attr_buffer = Vec::with_capacity(buf_size);
        attr_buffer.resize(buf_size, default_attr);

        Self {
            size,
            text_buffer: UnsafeCell::new(text_buffer.into_boxed_slice()),
            attr_buffer: UnsafeCell::new(attr_buffer.into_boxed_slice()),
        }
    }

    #[inline]
    pub fn view<'a, 'b>(&'b self) -> TextBufferView<'a, TCHAR>
    where
        'b: 'a,
    {
        TextBufferView {
            buffer: self,
            frame: Rect::new(Point::new(0, 0), self.size),
        }
    }

    #[inline]
    pub fn sub_view<'a, 'b>(&'b self, region: Rect) -> Option<TextBufferView<'a, TCHAR>>
    where
        'b: 'a,
    {
        let view = self.view();
        view.sub_view(region)
    }

    #[inline]
    pub fn point_to_index(&self, pos: Point) -> Option<usize> {
        if pos.x < 0 || pos.y < 0 || pos.x >= self.size.width || pos.y >= self.size.height {
            return None;
        }
        Some((pos.y as usize * self.size.width as usize) + (pos.x as usize))
    }

    #[inline]
    pub fn get(&self, index: usize) -> Option<(TCHAR, TuiAttribute)> {
        unsafe {
            // SAFETY: I believe that each element of the array is safe to access simultaneously.
            let text_buffer = &*self.text_buffer.get();
            let attr_buffer = &*self.attr_buffer.get();

            let ch = text_buffer.get(index)?;
            let attr = attr_buffer.get(index)?;

            Some((
                (ch as *const TCHAR).read_volatile(),
                (attr as *const TuiAttribute).read_volatile(),
            ))
        }
    }

    #[inline]
    pub fn set(&self, index: usize, ch: TCHAR, attr: TuiAttribute) -> Option<()> {
        unsafe {
            // SAFETY: I believe that each element of the array is safe to access simultaneously.
            let text_buffer = &mut *self.text_buffer.get();
            let attr_buffer = &mut *self.attr_buffer.get();

            let text_cell = text_buffer.get_mut(index)?;
            let attr_cell = attr_buffer.get_mut(index)?;

            (text_cell as *mut TCHAR).write_volatile(ch);
            (attr_cell as *mut TuiAttribute).write_volatile(attr);
            Some(())
        }
    }
}

impl<TCHAR: TChar> TextBufferDrawing<TCHAR> for TextBuffer<TCHAR> {
    #[inline]
    fn put_char_at(&mut self, pos: Point, ch: TCHAR, attr: TuiAttribute) -> Option<()> {
        let idx = self.point_to_index(pos)?;
        self.set(idx, ch, attr)
    }

    #[inline]
    fn get_char_at(&self, pos: Point) -> Option<(TCHAR, TuiAttribute)> {
        let idx = self.point_to_index(pos)?;
        self.get(idx)
    }

    fn draw_hline(&mut self, origin: Point, length: i32, ch: TCHAR, attr: TuiAttribute) {
        let origin_idx = match self.point_to_index(origin) {
            Some(idx) => idx,
            None => return,
        };
        let right = ((origin.x + length - 1) as usize)
            .min((self.size.width as usize).saturating_sub(1)) as i32;
        let right_idx = match self.point_to_index(Point::new(right, origin.y)) {
            Some(idx) => idx,
            None => return,
        };
        for idx in origin_idx..=right_idx {
            self.set(idx, ch, attr);
        }
    }
}

impl<'a, TCHAR: TChar> TextBufferView<'a, TCHAR> {
    #[inline]
    pub fn sub_view(&self, region: Rect) -> Option<TextBufferView<'a, TCHAR>> {
        let new_frame = region + self.frame.top_left();

        let outer_bottom_right = self.frame.bottom_right()?;
        let inner_bottom_right = new_frame.bottom_right()?;
        if new_frame.top_left().x < self.frame.top_left().x
            || new_frame.top_left().y < self.frame.top_left().y
            || inner_bottom_right.x > outer_bottom_right.x
            || inner_bottom_right.y > outer_bottom_right.y
        {
            return None;
        }

        Some(TextBufferView {
            buffer: self.buffer,
            frame: new_frame,
        })
    }
}

impl<TCHAR: TChar> TextBufferView<'_, TCHAR> {
    #[inline]
    pub fn point_to_index(&self, pos: Point) -> Option<usize> {
        let origin = self.frame.top_left();
        if pos.x < origin.x
            || pos.y < origin.y
            || pos.x >= self.frame.size().width
            || pos.y >= self.frame.size().height
        {
            return None;
        }
        self.buffer.point_to_index(origin + pos)
    }
}

impl<TCHAR: TChar> TextBufferDrawing<TCHAR> for TextBufferView<'_, TCHAR> {
    #[inline]
    fn put_char_at(&mut self, pos: Point, ch: TCHAR, attr: TuiAttribute) -> Option<()> {
        let idx = self.point_to_index(pos)?;
        self.buffer.set(idx, ch, attr)
    }

    #[inline]
    fn get_char_at(&self, pos: Point) -> Option<(TCHAR, TuiAttribute)> {
        let idx = self.point_to_index(pos)?;
        self.buffer.get(idx)
    }
}
