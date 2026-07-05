use std::sync::Arc;
#[cfg(not(feature = "ziqa-bga-direct"))]
use std::time::Instant;

use log::error;
use orbclient::rect::Rect;
use orbclient::{Color, image::Image};

use crate::core::display::{Display, Displays};

#[cfg(feature = "ziqa-bga-direct")]
const CURSOR_SIZE: u32 = 15;
#[cfg(feature = "ziqa-bga-direct")]
const CURSOR_HOT_X: i32 = 7;
#[cfg(feature = "ziqa-bga-direct")]
const CURSOR_HOT_Y: i32 = 7;
#[cfg(feature = "ziqa-bga-direct")]
const CURSOR_DAMAGE_PAD: i32 = 1;

pub struct Compositor {
    displays: Displays,

    redraws: Vec<Rect>,

    #[cfg(not(feature = "ziqa-bga-direct"))]
    hw_cursor: bool,
    damage_borders: bool,
    // QEMU UIs do not grab the pointer in case an absolute pointing device is present
    // and since releasing our gpu cursor makes it disappear, updating it every second fixes it
    #[cfg(not(feature = "ziqa-bga-direct"))]
    update_cursor_timer: Instant,
    cursor: Arc<Image>,
    cursor_x: i32,
    cursor_y: i32,
    cursor_hot_x: i32,
    cursor_hot_y: i32,
}

impl Compositor {
    pub fn new(displays: Displays) -> Self {
        Compositor {
            displays,
            redraws: Vec::new(),
            #[cfg(not(feature = "ziqa-bga-direct"))]
            hw_cursor: true,
            damage_borders: false,
            #[cfg(not(feature = "ziqa-bga-direct"))]
            update_cursor_timer: Instant::now(),
            cursor: Arc::new(Image::new(0, 0)),
            cursor_x: 0,
            cursor_y: 0,
            cursor_hot_x: 0,
            cursor_hot_y: 0,
        }
    }

    pub fn displays(&self) -> &[Display] {
        &self.displays.displays
    }

    pub fn toggle_damage_border(&mut self) {
        self.damage_borders = !self.damage_borders;
    }

    /// Return the first display screen rectangle
    pub fn screen_rect(&self) -> Rect {
        self.displays()[0].screen_rect()
    }

    /// Return the first display scale rectangle
    pub fn scale(&self) -> u32 {
        self.displays()[0].scale()
    }

    /// Return the first display factored scale
    pub fn factored_scale(&self) -> u32 {
        self.displays()[0].factored_scale()
    }

    /// Find the display that a window (`rect`) most overlaps and return it's screen_rect
    pub fn get_screen_rect_for_window(&self, rect: &Rect) -> Rect {
        let mut best_display = &self.displays()[0];
        let mut best_overlap = 0;

        for display in self.displays() {
            let overlap = rect.intersection(&display.screen_rect()).area();
            if overlap > best_overlap {
                best_overlap = overlap;
                best_display = display;
            }
        }

        best_display.screen_rect()
    }

    /// Reduce the rect height based on orblauncher bar height
    pub fn get_window_rect_from_screen_rect(&self, screen_rect: &Rect) -> Rect {
        let bar_height = (screen_rect.height() as f32 * 0.04) as u32;
        Rect::new(
            screen_rect.left(),
            screen_rect.top(),
            screen_rect.width(),
            screen_rect.height().saturating_sub(bar_height as u32),
        )
    }

    pub fn resize_if_necessary(&mut self) -> bool {
        // TODO: should screens be moved after a resize?
        let mut any_resized = false;
        #[cfg(not(feature = "ziqa-bga-direct"))]
        {
            for i in 0..self.displays.displays.len() {
                let resized =
                    self.displays.displays[i].resize_if_necessary(&self.displays.display_handle);
                any_resized |= resized;
                if resized {
                    self.schedule(self.displays.displays[i].screen_rect());
                }
            }
        }
        #[cfg(feature = "ziqa-bga-direct")]
        {
            for i in 0..self.displays.displays.len() {
                let resized = self.displays.displays[i].resize_if_necessary(&());
                any_resized |= resized;
                if resized {
                    self.schedule(self.displays.displays[i].screen_rect());
                }
            }
        }
        any_resized
    }

    pub fn schedule(&mut self, request: Rect) {
        if request.is_empty() {
            return;
        }

        #[cfg(feature = "ziqa-bga-direct")]
        let request = match Self::clip_rect_to_rect(&request, &self.screen_rect()) {
            Some(request) => request,
            None => return,
        };

        for rect in self.redraws.iter_mut() {
            // If contained, ignore new redraw request
            let container = rect.container(&request);
            if container.width() == request.width() && container.height() == request.height() {
                *rect = container;
                return;
            }
        }

        self.redraws.push(request);
    }

    /// True when at least one rectangle is pending a redraw this frame.
    /// Used by the main loop to pick active (60fps) vs idle (~10Hz) pacing
    /// without forcing a full `redraw()` to discover it.
    pub fn is_dirty(&self) -> bool {
        !self.redraws.is_empty()
    }

    fn cursor_rect(&self) -> Rect {
        #[cfg(feature = "ziqa-bga-direct")]
        {
            return Self::cursor_overlay_rect(self.cursor_x, self.cursor_y);
        }

        #[cfg(not(feature = "ziqa-bga-direct"))]
        {
            Rect::new(
                self.cursor_x - self.cursor_hot_x,
                self.cursor_y - self.cursor_hot_y,
                self.cursor.width(),
                self.cursor.height(),
            )
        }
    }

    #[cfg(feature = "ziqa-bga-direct")]
    fn cursor_overlay_rect(x: i32, y: i32) -> Rect {
        Rect::new(x - CURSOR_HOT_X, y - CURSOR_HOT_Y, CURSOR_SIZE, CURSOR_SIZE)
    }

    #[cfg(feature = "ziqa-bga-direct")]
    fn pad_rect(rect: Rect, pad: i32) -> Rect {
        Rect::new(
            rect.left() - pad,
            rect.top() - pad,
            rect.width() + (pad as u32 * 2),
            rect.height() + (pad as u32 * 2),
        )
    }

    #[cfg(feature = "ziqa-bga-direct")]
    fn clip_rect_to_rect(rect: &Rect, clip: &Rect) -> Option<Rect> {
        let left = rect.left().max(clip.left());
        let top = rect.top().max(clip.top());
        let right = rect.right().min(clip.right());
        let bottom = rect.bottom().min(clip.bottom());

        if right <= left || bottom <= top {
            return None;
        }

        Some(Rect::new(
            left,
            top,
            (right - left) as u32,
            (bottom - top) as u32,
        ))
    }

    #[cfg(feature = "ziqa-bga-direct")]
    fn schedule_cursor_damage(&mut self, old_cursor_rect: Rect, new_cursor_rect: Rect) {
        let screen = self.screen_rect();
        let old_cursor_rect = Self::clip_rect_to_rect(&old_cursor_rect, &screen);
        let new_cursor_rect = Self::clip_rect_to_rect(&new_cursor_rect, &screen);
        if let Some(old_cursor_rect) = old_cursor_rect {
            let padded = Self::pad_rect(old_cursor_rect, CURSOR_DAMAGE_PAD);
            if let Some(clipped) = Self::clip_rect_to_rect(&padded, &screen) {
                self.schedule(clipped);
            }
        }
        if let Some(new_cursor_rect) = new_cursor_rect {
            let padded = Self::pad_rect(new_cursor_rect, CURSOR_DAMAGE_PAD);
            if let Some(clipped) = Self::clip_rect_to_rect(&padded, &screen) {
                self.schedule(clipped);
            }
        }
    }

    #[cfg(feature = "ziqa-bga-direct")]
    fn draw_cursor_rect(display: &mut Display, clip: &Rect, rect: Rect, color: Color) {
        if let Some(clipped) = Self::clip_rect_to_rect(&rect, clip) {
            display.rect(&clipped, color);
        }
    }

    #[cfg(feature = "ziqa-bga-direct")]
    fn draw_cursor_diamond(
        display: &mut Display,
        clip: &Rect,
        cx: i32,
        cy: i32,
        r: i32,
        color: Color,
    ) {
        for dy in -r..=r {
            let width = (r - dy.abs()) * 2 + 1;
            Self::draw_cursor_rect(
                display,
                clip,
                Rect::new(cx - width / 2, cy + dy, width as u32, 1),
                color,
            );
        }
    }

    #[cfg(feature = "ziqa-bga-direct")]
    fn draw_cursor_overlay(display: &mut Display, clip: &Rect, x: i32, y: i32) {
        let black = Color::rgb(0x00, 0x00, 0x00);
        let white = Color::rgb(0xFF, 0xFF, 0xFF);
        let cyan = Color::rgb(0x00, 0xC8, 0xFF);

        Self::draw_cursor_diamond(display, clip, x, y, 5, black);
        Self::draw_cursor_diamond(display, clip, x, y, 4, white);
        Self::draw_cursor_rect(display, clip, Rect::new(x - 1, y - 1, 3, 3), cyan);
    }

    pub fn update_cursor(&mut self, x: i32, y: i32, hot_x: i32, hot_y: i32, cursor: &Arc<Image>) {
        #[cfg(feature = "ziqa-bga-direct")]
        {
            let old_cursor_rect = self.cursor_rect();
            self.cursor_x = x;
            self.cursor_y = y;
            self.cursor_hot_x = hot_x;
            self.cursor_hot_y = hot_y;
            self.cursor = cursor.clone();
            let new_cursor_rect = self.cursor_rect();
            self.schedule_cursor_damage(old_cursor_rect, new_cursor_rect);
            return;
        }

        #[cfg(not(feature = "ziqa-bga-direct"))]
        {
            if !self.hw_cursor {
                self.schedule(self.cursor_rect());
            }

            if self.hw_cursor {
                if Arc::ptr_eq(&self.cursor, cursor)
                    && self.cursor_hot_x == hot_x
                    && self.cursor_hot_y == hot_y
                {
                    match self.displays.displays[0].move_cursor(&self.displays.display_handle, x, y)
                    {
                        Ok(_) => (),
                        Err(err) => error!("failed to move cursor: {}", err),
                    }
                } else {
                    match self.displays.displays[0].set_cursor(
                        &self.displays.display_handle,
                        hot_x,
                        hot_y,
                        cursor,
                    ) {
                        Ok(_) => (),
                        Err(err) => error!("failed to update cursor: {}", err),
                    }

                    match self.displays.displays[0].move_cursor(&self.displays.display_handle, x, y)
                    {
                        Ok(_) => (),
                        Err(err) => error!("failed to move cursor: {}", err),
                    }
                }
            }

            self.cursor_x = x;
            self.cursor_y = y;
            self.cursor_hot_x = hot_x;
            self.cursor_hot_y = hot_y;
            self.cursor = cursor.clone();

            if !self.hw_cursor {
                self.schedule(self.cursor_rect());
            }
        }
    }

    pub fn redraw(&mut self, draw_windows: impl FnMut(&mut Display, Rect)) {
        #[cfg(feature = "ziqa-bga-direct")]
        {
            self.redraw_direct_bga(draw_windows);
            return;
        }

        #[cfg(not(feature = "ziqa-bga-direct"))]
        {
            let total_redraw_opt = self.redraw_windows(draw_windows);
            self.redraw_cursor(total_redraw_opt);

            // Sync any parts of displays that changed
            self.sync_rect(total_redraw_opt.unwrap_or(Rect::new(0, 0, 0, 0)));
        }
    }
    
    #[cfg(feature = "ziqa-bga-direct")]
    fn redraw_direct_bga(&mut self, mut draw_windows: impl FnMut(&mut Display, Rect)) {
        let raw_cursor_rect = self.cursor_rect();
        let redraws = std::mem::take(&mut self.redraws);

        // PROF-TEMP: per-frame sub-phase accumulators (microseconds)
        let mut dw_us: u64 = 0;
        let mut cur_us: u64 = 0;
        let mut syn_us: u64 = 0;
        let mut dirty_area: u64 = 0;
        let mut rect_count: u32 = 0;

        for original_rect in redraws {
            if original_rect.is_empty() {
                continue;
            }
            // PROF-TEMP: dirty scope (rects are already screen-clipped by schedule)
            rect_count += 1;
            dirty_area += (original_rect.width() as u64) * (original_rect.height() as u64);

            for (i, display) in self.displays.displays.iter_mut().enumerate() {
                let screen = display.screen_rect();
                let Some(rect) = Self::clip_rect_to_rect(&original_rect, &screen) else {
                    continue;
                };

                // PROF-TEMP: time the window-compositing closure
                let _t = crate::prof::tsc();
                draw_windows(display, rect);
                dw_us += crate::prof::since(_t);

                if let Some(cursor_rect) = Self::clip_rect_to_rect(&raw_cursor_rect, &screen) {
                    if Self::clip_rect_to_rect(&cursor_rect, &rect).is_some() {
                        // PROF-TEMP: time software cursor overlay redraw
                        let _t = crate::prof::tsc();
                        Self::draw_cursor_overlay(display, &rect, self.cursor_x, self.cursor_y);
                        cur_us += crate::prof::since(_t);
                    }
                }

                if self.damage_borders {
                    const DAMAGE_COLOR: Color = Color::rgba(255, 0, 255, 80);
                    display.border_rect(&rect, DAMAGE_COLOR, 2);
                }
                // PROF-TEMP: time the BGA backbuffer->scanout flush
                let _t = crate::prof::tsc();
                match display.sync_rect(&(), &rect) {
                    Ok(()) => (),
                    Err(err) => error!("failed to sync display {}: {}", i, err),
                }
                syn_us += crate::prof::since(_t);
            }
        }

        // PROF-TEMP: publish this frame's compositor breakdown (no-op when idle)
        if rect_count > 0 {
            crate::prof::add_drawwin(dw_us);
            crate::prof::add_cursor(cur_us);
            crate::prof::add_sync(syn_us, dirty_area, rect_count);
        }
    }

    #[cfg(not(feature = "ziqa-bga-direct"))]
    fn redraw_windows(&mut self, mut draw_windows: impl FnMut(&mut Display, Rect)) -> Option<Rect> {
        let mut total_redraw_opt: Option<Rect> = None;

        // go through the list of rectangles pending a redraw and expand the total redraw rectangle
        // to encompass all of them
        for original_rect in self.redraws.drain(..) {
            if !original_rect.is_empty() {
                total_redraw_opt = Some(
                    total_redraw_opt
                        .unwrap_or(original_rect)
                        .container(&original_rect),
                );
            }

            for display in self.displays.displays.iter_mut() {
                let rect = original_rect.intersection(&display.screen_rect());
                if rect.is_empty() {
                    continue;
                }

                draw_windows(display, rect);
            }
        }

        total_redraw_opt
    }

    #[cfg(not(feature = "ziqa-bga-direct"))]
    fn redraw_cursor(&mut self, total_redraw: Option<Rect>) {
        #[cfg(feature = "ziqa-bga-direct")]
        {
            let Some(total_redraw) = total_redraw else {
                return;
            };

            let raw_cursor_rect = self.cursor_rect();
            for display in self.displays.displays.iter_mut() {
                let screen = display.screen_rect();
                let Some(rect) = Self::clip_rect_to_rect(&total_redraw, &screen) else {
                    continue;
                };
                let Some(cursor_rect) = Self::clip_rect_to_rect(&raw_cursor_rect, &screen) else {
                    continue;
                };

                if Self::clip_rect_to_rect(&cursor_rect, &rect).is_some() {
                    Self::draw_cursor_overlay(display, &rect, self.cursor_x, self.cursor_y);
                }
            }
            return;
        }

        #[cfg(not(feature = "ziqa-bga-direct"))]
        {
            if self.hw_cursor {
                if self.update_cursor_timer.elapsed().as_millis() > 1000 {
                    match self.displays.displays[0].set_cursor(
                        &self.displays.display_handle,
                        self.cursor_hot_x,
                        self.cursor_hot_y,
                        &self.cursor,
                    ) {
                        Ok(_) => (),
                        Err(err) => error!("failed to update cursor: {}", err),
                    }

                    match self.displays.displays[0].move_cursor(
                        &self.displays.display_handle,
                        self.cursor_x,
                        self.cursor_y,
                    ) {
                        Ok(_) => (),
                        Err(err) => error!("failed to move cursor: {}", err),
                    }

                    self.update_cursor_timer = Instant::now();
                }

                return;
            }

            let Some(total_redraw) = total_redraw else {
                return;
            };

            let cursor_rect = self.cursor_rect();

            for display in self.displays.displays.iter_mut() {
                let rect = total_redraw.intersection(&display.screen_rect());
                if !rect.is_empty() {
                    let cursor_intersect = rect.intersection(&cursor_rect);
                    if !cursor_intersect.is_empty() {
                        display.roi_mut(&cursor_intersect).blend(&self.cursor.roi(
                            &cursor_intersect.translate(-cursor_rect.left(), -cursor_rect.top()),
                        ));
                    }
                }
            }
        }
    }

    #[cfg(not(feature = "ziqa-bga-direct"))]
    pub fn sync_rect(&mut self, total_redraw: Rect) {
        // Sync any parts of displays that changed
        for (i, display) in self.displays.displays.iter_mut().enumerate() {
            let display_redraw = total_redraw.intersection(&display.screen_rect());
            if !display_redraw.is_empty() {
                if self.damage_borders {
                    const DAMAGE_COLOR: Color = Color::rgba(255, 0, 255, 80);
                    display.border_rect(&display_redraw, DAMAGE_COLOR, 2);
                }
                #[cfg(not(feature = "ziqa-bga-direct"))]
                match display.sync_rect(&self.displays.display_handle, display_redraw) {
                    Ok(()) => (),
                    Err(err) => error!("failed to sync display {}: {}", i, err),
                }
                #[cfg(feature = "ziqa-bga-direct")]
                match display.sync_rect(&(), &display_redraw) {
                    Ok(()) => (),
                    Err(err) => error!("failed to sync display {}: {}", i, err),
                }
            }
        }
    }
}
