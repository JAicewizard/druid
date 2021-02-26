// Copyright 2019 The Druid Authors.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! GTK window creation and management.

use std::any::Any;
use std::cell::{Cell, RefCell};
use std::convert::{TryFrom};
use std::panic::Location;
use std::sync::{Arc, Mutex, Weak};
use std::time::Instant;
use keyboard_types::Key;
use anyhow::anyhow;
use gdk::{ModifierType,Surface};
use gdk::keys::Key as GDKKey;
use gtk::prelude::*;
use gtk::glib::signal::Inhibit;
use gtk::{ApplicationWindow, DrawingArea, PopoverExt,EventControllerExt};
use gtk::cairo;
use tracing::{error, warn};
use gtk::gdk_pixbuf::{Pixbuf,Colorspace};
#[cfg(feature = "raw-win-handle")]
use raw_window_handle::{HasRawWindowHandle, RawWindowHandle};

use crate::kurbo::{Insets, Point, Rect, Size, Vec2};
use crate::piet::{Piet, PietText, RenderContext};

use crate::common_util::{ClickCounter, IdleCallback};
use crate::dialog::{FileDialogOptions, FileDialogType, FileInfo};
use crate::error::Error as ShellError;
use crate::keyboard::{KeyEvent, KeyState, Modifiers};
use crate::mouse::{Cursor, CursorDesc, MouseButton, MouseButtons, MouseEvent};
use crate::piet::ImageFormat;
use crate::region::Region;
use crate::scale::{Scalable, Scale, ScaledArea};
use crate::window;
use crate::window::{FileDialogToken, IdleToken, TimerToken, WinHandler, WindowLevel};

use super::application::Application;
use super::dialog;
use super::keycodes;
use super::menu::Menu;
use super::util;

/// The platform target DPI.
///
/// GTK considers 96 the default value which represents a 1.0 scale factor.
const SCALE_TARGET_DPI: f64 = 1.0;

/// Taken from https://gtk-rs.org/docs-src/tutorial/closures
/// It is used to reduce the boilerplate of setting up gtk callbacks
/// Example:
/// ```
/// button.connect_clicked(clone!(handle => move |_| { ... }))
/// ```
/// is equivalent to:
/// ```
/// {
///     let handle = handle.clone();
///     button.connect_clicked(move |_| { ... })
/// }
/// ```
macro_rules! clone {
    (@param _) => ( _ );
    (@param $x:ident) => ( $x );
    ($($n:ident),+ => move || $body:expr) => (
        {
            $( let $n = $n.clone(); )+
            move || $body
        }
    );
    ($($n:ident),+ => move |$($p:tt),+| $body:expr) => (
        {
            $( let $n = $n.clone(); )+
            move |$(clone!(@param $p),)+| $body
        }
    );
}

#[derive(Clone, Default)]
pub struct WindowHandle {
    pub(crate) state: Weak<WindowState>,
    // Ensure that we don't implement Send, because it isn't actually safe to send the WindowState.
    marker: std::marker::PhantomData<*const ()>,
}

#[cfg(feature = "raw-win-handle")]
unsafe impl HasRawWindowHandle for WindowHandle {
    fn raw_window_handle(&self) -> RawWindowHandle {
        error!("HasRawWindowHandle trait not implemented for gtk.");
        // GTK is not a platform, and there's no empty generic handle. Pick XCB randomly as fallback.
        RawWindowHandle::Xcb(XcbHandle::empty())
    }
}

/// Operations that we defer in order to avoid re-entrancy. See the documentation in the windows
/// backend for more details.
enum DeferredOp {
    SaveAs(FileDialogOptions, FileDialogToken),
    Open(FileDialogOptions, FileDialogToken),
    ContextMenu(Menu, WindowHandle),
}

/// Builder abstraction for creating new windows
pub(crate) struct WindowBuilder {
    app: Application,
    handler: Option<Box<dyn WinHandler>>,
    title: String,
    menu: Option<Menu>,
    position: Option<Point>,
    level: Option<WindowLevel>,
    state: Option<window::WindowState>,
    size: Size,
    min_size: Option<Size>,
    resizable: bool,
    show_titlebar: bool,
    transparent: bool,
}

#[derive(Clone)]
pub struct IdleHandle {
    idle_queue: Arc<Mutex<Vec<IdleKind>>>,
    state: Weak<WindowState>,
}

/// This represents different Idle Callback Mechanism
enum IdleKind {
    Callback(Box<dyn IdleCallback>),
    Token(IdleToken),
}

// We use RefCells for interior mutability, but we try to structure things so that double-borrows
// are impossible. See the documentation on crate::platform::x11::window::Window for more details,
// since the idea there is basically the same.
pub(crate) struct WindowState {
    window: ApplicationWindow,
    scale: Cell<Scale>,
    area: Cell<ScaledArea>,
    is_transparent: Cell<bool>,
    /// Used to determine whether to honor close requests from the system: we inhibit them unless
    /// this is true, and this gets set to true when our client requests a close.
    closing: Cell<bool>,
    key_event_controler: gtk::EventControllerKey,
    focus_event_controler: gtk::EventControllerFocus,
    click_controller: gtk::GestureClick,
    motion_controller: gtk::EventControllerMotion,

    drawing_area: DrawingArea,
    // A cairo surface for us to render to; we copy this to the drawing_area whenever necessary.
    // This extra buffer is necessitated by DrawingArea's painting model: when our paint callback
    // is called, we are given a cairo context that's already clipped to the invalid region. This
    // doesn't match up with our painting model, because we need to call `prepare_paint` before we
    // know what the invalid region is.
    //
    // The way we work around this is by always invalidating the entire DrawingArea whenever we
    // need repainting; this ensures that GTK gives us an unclipped cairo context. Meanwhile, we
    // keep track of the actual invalid region. We use that region to render onto `surface`, which
    // we then copy onto `drawing_area`.
    surface: RefCell<Option<Surface>>,
    // The size of `surface` in pixels. This could be bigger than `drawing_area`.
    surface_size: Cell<(i32, i32)>,
    // The invalid region, in display points.
    invalid: RefCell<Region>,
    pub(crate) handler: RefCell<Box<dyn WinHandler>>,
    idle_queue: Arc<Mutex<Vec<IdleKind>>>,
    current_keycode: Cell<Option<u32>>, //actually a v
    click_counter: ClickCounter,
    deferred_queue: RefCell<Vec<DeferredOp>>,
}

#[derive(Clone, PartialEq)]
pub struct CustomCursor(gdk::Cursor);

impl WindowBuilder {
    pub fn new(app: Application) -> WindowBuilder {
        WindowBuilder {
            app,
            handler: None,
            title: String::new(),
            menu: None,
            size: Size::new(500.0, 400.0),
            position: None,
            level: None,
            state: None,
            min_size: None,
            resizable: true,
            show_titlebar: true,
            transparent: false,
        }
    }

    pub fn set_handler(&mut self, handler: Box<dyn WinHandler>) {
        self.handler = Some(handler);
    }

    pub fn set_size(&mut self, size: Size) {
        self.size = size;
    }

    pub fn set_min_size(&mut self, size: Size) {
        self.min_size = Some(size);
    }

    pub fn resizable(&mut self, resizable: bool) {
        self.resizable = resizable;
    }

    pub fn show_titlebar(&mut self, show_titlebar: bool) {
        self.show_titlebar = show_titlebar;
    }

    pub fn set_transparent(&mut self, transparent: bool) {
        self.transparent = transparent;
    }

    pub fn set_position(&mut self, position: Point) {
        self.position = Some(position);
    }

    pub fn set_level(&mut self, level: WindowLevel) {
        self.level = Some(level);
    }

    pub fn set_window_state(&mut self, state: window::WindowState) {
        self.state = Some(state);
    }

    pub fn set_title(&mut self, title: impl Into<String>) {
        self.title = title.into();
    }

    pub fn set_menu(&mut self, menu: Menu) {
        self.menu = Some(menu);
    }

    pub fn build(self) -> Result<WindowHandle, ShellError> {
        println!("NEWDINW");

        let handler = self
            .handler
            .expect("Tried to build a window without setting the handler");

        let window = ApplicationWindow::new(self.app.gtk_app());
        println!("NEWDINW2");

        window.set_title(Some(&self.title));
        window.set_resizable(self.resizable);
        //window.set_app_paintable(true);
        window.set_decorated(self.show_titlebar);
        let mut can_transparent = false;
        //FIXME: transparency is enabled by default.
        // if self.transparent {
        //     if let Some(screen) = window.get_screen() {
        //         let visual = screen.get_rgba_visual();
        //         can_transparent = visual.is_some();
        //         window.set_visual(visual.as_ref());
        //     }
        // }
        //FIXME: check if the scale factor is still correct
        // Get the scale factor based on the GTK reported DPI
        let mut value = gtk::glib::value::Value::from_type(gtk::glib::types::Type::I32);
        window.get_display().get_setting("gtk-xft-hinting",&mut value);
        let scale_factor = value.downcast::<i32>().unwrap().get_some()as f64 / SCALE_TARGET_DPI ;
        println!("SCALE FACTOR{:?}", scale_factor);
        let scale = Scale::new(scale_factor, scale_factor);
        let area = ScaledArea::from_dp(self.size, scale);
        let size_px = area.size_px();

        window.set_default_size(size_px.width as i32, size_px.height as i32);

        let vbox = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let key_event_controler = gtk::EventControllerKey::new();
        let focus_event_controler = gtk::EventControllerFocus::new();
        let click_controller = gtk::GestureClick::new();
        let motion_controller = gtk::EventControllerMotion::new();
        vbox.add_controller(&key_event_controler);
        vbox.add_controller(&focus_event_controler);
        vbox.add_controller(&click_controller);
        vbox.add_controller(&motion_controller);

        let drawing_area = gtk::DrawingArea::new();
        drawing_area.set_hexpand(true);
        drawing_area.set_hexpand_set(true);
        drawing_area.set_vexpand(true);
        drawing_area.set_vexpand_set(true);

        vbox.append(&drawing_area);
        vbox.set_hexpand(true);
        vbox.set_hexpand_set(true);
        vbox.set_vexpand(true);
        vbox.set_vexpand_set(true);
                        window.set_child(Some(&vbox));

        let win_state = Arc::new(WindowState {
            window,
            scale: Cell::new(scale),
            area: Cell::new(area),
            is_transparent: Cell::new(self.transparent & can_transparent),
            closing: Cell::new(false),
            key_event_controler,
            focus_event_controler,
            click_controller,
            drawing_area,
            motion_controller,
            surface: RefCell::new(None),
            surface_size: Cell::new((0, 0)),
            invalid: RefCell::new(Region::EMPTY),
            handler: RefCell::new(handler),
            idle_queue: Arc::new(Mutex::new(vec![])),
            current_keycode: Cell::new(None),
            click_counter: ClickCounter::default(),
            deferred_queue: RefCell::new(Vec::new()),
        });

        self.app
            .gtk_app()
            .connect_shutdown(clone!(win_state => move |_| {
                // this ties a clone of Arc<WindowState> to the ApplicationWindow to keep it alive
                // when the ApplicationWindow is destroyed, the last Arc is dropped
                // and any Weak<WindowState> will be None on upgrade()
                let _ = &win_state;
            }));

        let mut handle = WindowHandle {
            state: Arc::downgrade(&win_state),
            marker: std::marker::PhantomData,
        };
        if let Some(level) = self.level {
            handle.set_level(level);
        }
        if let Some(pos) = self.position {
            handle.set_position(pos);
        }
        if let Some(state) = self.state {
            handle.set_window_state(state)
        }

        if let Some(menu) = self.menu {
            let menu = menu.into_gtk_menubar(&handle);
            vbox.prepend(&menu);
        }

        win_state.drawing_area.set_can_focus(true);
        win_state.drawing_area.grab_focus();


        if let Some(min_size_dp) = self.min_size {
            let min_area = ScaledArea::from_dp(min_size_dp, scale);
            let min_size_px = min_area.size_px();
            win_state
                .drawing_area
                .set_size_request(min_size_px.width as i32, min_size_px.height as i32);
        }
        win_state
            .motion_controller
            .connect_enter(|focus, x, y| {
                focus.get_widget().unwrap().grab_focus();
            });

        win_state.motion_controller.connect_leave(
            clone!(handle => move |focus| {
                if let Some(state) = handle.state.upgrade() {
                    let scale = state.scale.get();
                    let crossing_state = focus.get_current_event_state();
                    let point: Point = (0.,0.).into();
                    let mouse_event = MouseEvent {
                        pos: point.to_dp(scale),
                        buttons: get_mouse_buttons_from_modifiers(crossing_state),
                        mods: get_modifiers(Some(crossing_state)),
                        count: 0,
                        focus: false,
                        button: MouseButton::None,
                        wheel_delta: Vec2::ZERO
                    };

                    state.with_handler(|h| h.mouse_move(&mouse_event));
                }
            }),
        );

        win_state.drawing_area.set_draw_func(clone!(handle => move |drawing_area, context, width, height| {
            if let Some(state) = handle.state.upgrade() {
                let mut scale = state.scale.get();
                let mut scale_changed = false;
                // Check if the GTK reported DPI has changed,
                // so that we can change our scale factor without restarting the application.
                let mut value = gtk::glib::value::Value::from_type(gtk::glib::types::Type::I32);
                state.window.get_display().get_setting("gtk-xft-hinting",&mut value);
                let scale_factor = value.downcast::<i32>().unwrap().get_some()as f64 / SCALE_TARGET_DPI ;
                let reported_scale = Scale::new(scale_factor, scale_factor);
                if scale != reported_scale {
                    scale = reported_scale;
                    state.scale.set(scale);
                    scale_changed = true;
                    state.with_handler(|h| h.scale(scale));
                }
                

                // Create a new cairo surface if necessary (either because there is no surface, or
                // because the size or scale changed).
                let size_px = Size::new(width as f64, height as f64);
                let no_surface = state.surface.try_borrow().map(|x| x.is_none()).ok() == Some(true);
                if no_surface || scale_changed || state.area.get().size_px() != size_px {
                    let area = ScaledArea::from_px(size_px, scale);
                    let size_dp = area.size_dp();
                    state.area.set(area);
                    if let Err(e) = state.resize_surface(width, height) {
                        error!("Failed to resize surface: {}", e);
                    }
                    state.with_handler(|h| h.size(size_dp));
                    state.invalidate_rect(size_dp.to_rect());
                }

                state.with_handler(|h| h.prepare_paint());

                let invalid = match state.invalid.try_borrow_mut() {
                    Ok(mut invalid) => std::mem::replace(&mut *invalid, Region::EMPTY),
                    Err(_) => {
                        error!("invalid region borrowed while drawing");
                        Region::EMPTY
                    }
                };

                if let Ok(Some(surface)) = state.surface.try_borrow().as_ref().map(|s| s.as_ref()) {
                    // Note that we're borrowing the surface while calling the handler. This is ok,
                    // because we don't return control to the system or re-borrow the surface from
                    // any code that the client can call.
                    state.with_handler_and_dont_check_the_other_borrows(|handler| {
                        //TODO error?
                        let mut region = cairo::Region::create();
                        for rect in invalid.rects() {
                            println!("rect!{:?}",rect);
                            let rect = rect.to_px(scale);
                            let rect1  = cairo::RectangleInt{x:rect.x0 as i32,y:rect.y0 as i32,width:rect.width() as i32,height:rect.height() as i32};
                            region.union_rectangle(&rect1);
                        } 
                        if region.is_empty(){
                            println!("empty1!");

                            let rect1  = cairo::RectangleInt{x:0,y:0,width:width,height:height};

                            region.union_rectangle(&rect1);

                        }
                        println!("{:?}" ,region.get_rectangle(0));

                        let c_context = surface.create_cairo_context().unwrap();
                        c_context.begin_frame(&region);
                        //let surface_context = c_context.cairo_create().unwrap();

                        // Clip to the invalid region, in order that our surface doesn't get
                        // messed up if there's any painting outside them.
                        for rect in invalid.rects() {
                            let rect = rect.to_px(scale);
                            context.rectangle(rect.x0, rect.y0, rect.width(), rect.height());
                        }
                        context.clip();

                        context.scale(scale.x(), scale.y());
                        let mut piet_context = Piet::new(&context);
                        handler.paint(&mut piet_context, &invalid);
                        if let Err(e) = piet_context.finish() {
                            error!("piet error on render: {:?}", e);
                        }

                        // Copy the entir`e surface to the drawing area (not just the invalid
                        // region, because there might be parts of the drawing area that were
                        // invalidated by external forces).
                        c_context.end_frame();

                    });
                } else {
                    warn!("Drawing was skipped because there was no surface");
                }
            }
        }));
        //TODO: is this still needed?
        // win_state.drawing_area.connect_screen_changed(
        //     clone!(handle => move |widget, _prev_screen| {
        //         if let Some(state) = handle.state.upgrade() {

        //             if let Some(screen) = widget.get_screen(){
        //                 let visual = screen.get_rgba_visual();
        //                 state.is_transparent.set(visual.is_some());
        //                 widget.set_visual(visual.as_ref());
        //             }
        //         }
        //     }),
        // );

        win_state.click_controller.connect_pressed(clone!(handle => move |guesture, n, x, y| {
            if let Some(state) = handle.state.upgrade() {
                state.with_handler(|handler| {
                    if let Some(button) = get_mouse_button(guesture.get_button()) {
                        let scale = state.scale.get();
                        let button_state = guesture.get_current_event_state();
                        let pos: Point =  (x,y).into();
                        
                        //FIXME: there used to be a more complex counting system, not sure what it was for
                        if n == 0 || n == 1 {
                            handler.mouse_down(
                                &MouseEvent {
                                    pos: pos.to_dp(scale),
                                    buttons: get_mouse_buttons_from_modifiers(button_state).with(button),
                                    mods: get_modifiers(Some(button_state)),
                                    count: n as u8,
                                    focus: false,
                                    button,
                                    wheel_delta: Vec2::ZERO
                                },
                            );
                        }
                    }
                });
            }
        }));

        win_state.click_controller.connect_released(clone!(handle => move |guesture, n, x, y| {
            if let Some(state) = handle.state.upgrade() {
                state.with_handler(|handler| {
                    if let Some(button) = get_mouse_button(guesture.get_button()) {
                        let scale = state.scale.get();
                        let button_state = guesture.get_current_event_state();
                        handler.mouse_up(
                            &MouseEvent {
                                pos: Point::from((x,y)).to_dp(scale),
                                buttons: get_mouse_buttons_from_modifiers(button_state).without(button),
                                mods: get_modifiers(Some(button_state)),
                                count: 0,
                                focus: false,
                                button,
                                wheel_delta: Vec2::ZERO
                            },
                        );
                    }
                });
            }
        }));

        win_state.motion_controller.connect_motion(
            clone!(handle => move |motion, x, y| {
                if let Some(state) = handle.state.upgrade() {
                    let scale = state.scale.get();
                    let motion_state = motion.get_current_event_state();
                    let mouse_event = MouseEvent {
                        pos: Point::from((x,y)).to_dp(scale),
                        buttons: get_mouse_buttons_from_modifiers(motion_state),
                        mods: get_modifiers(Some(motion_state)),
                        count: 0,
                        focus: false,
                        button: MouseButton::None,
                        wheel_delta: Vec2::ZERO
                    };

                    state.with_handler(|h| h.mouse_move(&mouse_event));
                }
            }),
        );

        //TODO: viewport is needed for scrolling
        // win_state
        //     .drawing_area
        //     .connect_scroll_event(clone!(handle => move |_widget, scroll| {
        //         if let Some(state) = handle.state.upgrade() {
        //             let scale = state.scale.get();
        //             let mods = get_modifiers(scroll.get_state());

        //             // The magic "120"s are from Microsoft's documentation for WM_MOUSEWHEEL.
        //             // They claim that one "tick" on a scroll wheel should be 120 units.
        //             let shift = mods.shift();
        //             let wheel_delta = match scroll.get_direction() {
        //                 ScrollDirection::Up if shift => Some(Vec2::new(-120.0, 0.0)),
        //                 ScrollDirection::Up => Some(Vec2::new(0.0, -120.0)),
        //                 ScrollDirection::Down if shift => Some(Vec2::new(120.0, 0.0)),
        //                 ScrollDirection::Down => Some(Vec2::new(0.0, 120.0)),
        //                 ScrollDirection::Left => Some(Vec2::new(-120.0, 0.0)),
        //                 ScrollDirection::Right => Some(Vec2::new(120.0, 0.0)),
        //                 ScrollDirection::Smooth => {
        //                     //TODO: Look at how gtk's scroll containers implements it
        //                     let (mut delta_x, mut delta_y) = scroll.get_delta();
        //                     delta_x *= 120.;
        //                     delta_y *= 120.;
        //                     if shift {
        //                         delta_x += delta_y;
        //                         delta_y = 0.;
        //                     }
        //                     Some(Vec2::new(delta_x, delta_y))
        //                 }
        //                 e => {
        //                     eprintln!(
        //                         "Warning: the Druid widget got some whacky scroll direction {:?}",
        //                         e
        //                     );
        //                     None
        //                 }
        //             };

        //             if let Some(wheel_delta) = wheel_delta {
        //                 let mouse_event = MouseEvent {
        //                     pos: Point::from(scroll.get_position()).to_dp(scale),
        //                     buttons: get_mouse_buttons_from_modifiers(scroll.get_state()),
        //                     mods,
        //                     count: 0,
        //                     focus: false,
        //                     button: MouseButton::None,
        //                     wheel_delta
        //                 };

        //                 state.with_handler(|h| h.wheel(&mouse_event));
        //             }
        //         }

        //         Inhibit(true)
        //     }));

        win_state
            .key_event_controler
            .connect_key_pressed(clone!(handle => move |_controler, key, _u32, modi| {
                if let Some(state) = handle.state.upgrade() {

                    let repeat = state.current_keycode.get().clone() == Some(*key);

                    state.current_keycode.set(Some(*key));

                    state.with_handler(|h|
                        h.key_down(make_key_event(&key, repeat, KeyState::Down, Some(modi)))
                    );
                }

                Inhibit(true)
            }));

        win_state
            .key_event_controler
            .connect_key_released(clone!(handle => move |_controler, key, _u32, modi| {
                if let Some(state) = handle.state.upgrade() {

                    if state.current_keycode.get() == Some(*key) {
                        state.current_keycode.set(None);
                    }

                    state.with_handler(|h|
                        h.key_up(make_key_event(&key, false, KeyState::Up,Some(modi)))
                    );
                }
            }));
        win_state
            .focus_event_controler
            .connect_enter(clone!(handle => move |_focus| {
                if let Some(state) = handle.state.upgrade() {
                    state.with_handler(|h| h.got_focus());
                }
            }));

        win_state
            .focus_event_controler
            .connect_leave(clone!(handle => move |_focus| {
                if let Some(state) = handle.state.upgrade() {
                    state.with_handler(|h| h.lost_focus());
                }
            }));

        // win_state
        //     .window
        //     .connect_delete_event(clone!(handle => move |_widget, _ev| {
        //         if let Some(state) = handle.state.upgrade() {
        //             state.with_handler(|h| h.request_close());
        //             Inhibit(!state.closing.get())
        //         } else {
        //             Inhibit(false)
        //         }
        //     }));

        // win_state
        //     .drawing_area
        //     .connect_destroy(clone!(handle => move |_widget| {
        //         if let Some(state) = handle.state.upgrade() {
        //             state.with_handler(|h| h.destroy());
        //         }
        //     }));

        // win_state.drawing_area.realize();
        // win_state
        //     .drawing_area
        //     .get_window()
        //     .expect("realize didn't create window")
        //     .set_event_compression(false);

        let size = self.size;
        win_state.with_handler(|h| {
            h.connect(&handle.clone().into());
            h.scale(scale);
            h.size(size);
        });
        win_state.window.show();
        Ok(handle)
    }
}

impl WindowState {
    #[track_caller]
    fn with_handler<T, F: FnOnce(&mut dyn WinHandler) -> T>(&self, f: F) -> Option<T> {
        if self.invalid.try_borrow_mut().is_err() || self.surface.try_borrow_mut().is_err() {
            error!("other RefCells were borrowed when calling into the handler");
            return None;
        }

        let ret = self.with_handler_and_dont_check_the_other_borrows(f);

        self.run_deferred();
        ret
    }

    #[track_caller]
    fn with_handler_and_dont_check_the_other_borrows<T, F: FnOnce(&mut dyn WinHandler) -> T>(
        &self,
        f: F,
    ) -> Option<T> {
        match self.handler.try_borrow_mut() {
            Ok(mut h) => Some(f(&mut **h)),
            Err(_) => {
                error!("failed to borrow WinHandler at {}", Location::caller());
                None
            }
        }
    }

    fn resize_surface(&self, width: i32, height: i32) -> Result<(), anyhow::Error> {
        fn next_size(x: i32) -> i32 {
            // We round up to the nearest multiple of `accuracy`, which is between x/2 and x/4.
            // Don't bother rounding to anything smaller than 32 = 2^(7-1).
            let accuracy = 1 << ((32 - x.leading_zeros()).max(7) - 2);
            let mask = accuracy - 1;
            (x + mask) & !mask
        }

        let mut surface = self.surface.borrow_mut();
        let mut cur_size = self.surface_size.get();
        let (width, height) = (next_size(width), next_size(height));
        if surface.is_none() || cur_size != (width, height) {
            cur_size = (width, height);
            self.surface_size.set(cur_size);
            if let Some(s) = surface.as_ref() {
                s.destroy();
            }
            *surface = None;
            let display = self.window.get_display();
            if self.is_transparent.get() {
                *surface = Some(Surface::new_toplevel(&display));
            } else {
                *surface = Some(Surface::new_toplevel(&display));
            }
            if surface.is_none() {
                return Err(anyhow!("create_similar_surface failed"));
            }
        }
        Ok(())
    }

    /// Queues a call to `prepare_paint` and `paint`, but without marking any region for
    /// invalidation.
    fn request_anim_frame(&self) {
        self.window.queue_draw();
    }

    /// Invalidates a rectangle, given in display points.
    fn invalidate_rect(&self, rect: Rect) {
        if let Ok(mut region) = self.invalid.try_borrow_mut() {
            let scale = self.scale.get();
            // We prefer to invalidate an integer number of pixels.
            let rect = rect.to_px(scale).expand().to_dp(scale);
            region.add_rect(rect);
            self.window.queue_draw();
        } else {
            warn!("Not invalidating rect because region already borrowed");
        }
    }

    /// Pushes a deferred op onto the queue.
    fn defer(&self, op: DeferredOp) {
        self.deferred_queue.borrow_mut().push(op);
    }

    fn run_deferred(&self) {
        let queue = self.deferred_queue.replace(Vec::new());
        for op in queue {
            match op {
                DeferredOp::Open(options, token) => {
                    let file_info = dialog::get_file_dialog_path(
                        self.window.upcast_ref(),
                        FileDialogType::Open,
                        options,
                    )
                    .ok()
                    .map(|s| FileInfo { path: s.into() });
                    self.with_handler(|h| h.open_file(token, file_info));
                }
                DeferredOp::SaveAs(options, token) => {
                    let file_info = dialog::get_file_dialog_path(
                        self.window.upcast_ref(),
                        FileDialogType::Save,
                        options,
                    )
                    .ok()
                    .map(|s| FileInfo { path: s.into() });
                    self.with_handler(|h| h.save_as(token, file_info));
                }
                DeferredOp::ContextMenu(menu, handle) => {
                    let menu = menu.into_gtk_menu(&handle);
                    //menu.set_property_attach_widget(Some(&self.window));
                    menu.popup();
                }
            }
        }
    }
}

impl WindowHandle {
    pub fn show(&self) {
        //FIXME: What should this actually do? GTK4 shows all be default

        // if let Some(state) = self.state.upgrade() {
        //     state.window.show_all();
        // }
    }

    pub fn resizable(&self, resizable: bool) {
        if let Some(state) = self.state.upgrade() {
            state.window.set_resizable(resizable)
        }
    }

    pub fn show_titlebar(&self, show_titlebar: bool) {
        if let Some(state) = self.state.upgrade() {
            state.window.set_decorated(show_titlebar)
        }
    }

    pub fn set_position(&self, position: Point) {
        //FIXME: set_position is not a thing in gtk4
    }

    pub fn get_position(&self) -> Point {
        //FIXME: get_position is not a thing in gtk4
        Point::new(0.0, 0.0)
    }

    pub fn content_insets(&self) -> Insets {
        warn!("WindowHandle::content_insets unimplemented for GTK platforms.");
        Insets::ZERO
    }

    pub fn set_level(&self, level: WindowLevel) {
        //FIXME: Window hints are not a thing in gtk4
        // if let Some(state) = self.state.upgrade() {
        //     let hint = match level {
        //         WindowLevel::AppWindow => WindowTypeHint::Normal,
        //         WindowLevel::Tooltip => WindowTypeHint::Tooltip,
        //         WindowLevel::DropDown => WindowTypeHint::DropdownMenu,
        //         WindowLevel::Modal => WindowTypeHint::Dialog,
        //     };

        //     state.window.set_type_hint(hint);
        // }
    }

    pub fn set_size(&self, size: Size) {
        if let Some(state) = self.state.upgrade() {
            //FIXME: getting the window size is actually impossible!!!
            state.window.set_default_size(size.width as i32, size.height as i32)
        }
    }

    pub fn get_size(&self) -> Size {
        if let Some(state) = self.state.upgrade() {
            //FIXME: getting the window size is actually impossible!!!
            let (x, y) = state.window.get_default_size();
            Size::new(x as f64, y as f64)
        } else {
            warn!("Could not get size for GTK window");
            Size::new(0., 0.)
        }
    }

    pub fn set_window_state(&mut self, size_state: window::WindowState) {
        use window::WindowState::{MAXIMIZED, MINIMIZED, RESTORED};
        let cur_size_state = self.get_window_state();
        if let Some(state) = self.state.upgrade() {
            match (size_state, cur_size_state) {
                (s1, s2) if s1 == s2 => (),
                (MAXIMIZED, _) => state.window.maximize(),
                (MINIMIZED, _) => state.window.minimize(),
                (RESTORED, MAXIMIZED) => state.window.unmaximize(),
                (RESTORED, MINIMIZED) => state.window.unminimize(),
                (RESTORED, RESTORED) => (), // Unreachable
            }

            state.window.unmaximize();
        }
    }

    pub fn get_window_state(&self) -> window::WindowState {
        use window::WindowState::{MAXIMIZED, MINIMIZED, RESTORED};
        if let Some(state) = self.state.upgrade() {
            if state.window.is_maximized() {
                 MAXIMIZED
            } else {
                 MINIMIZED
            }
        }else{
            RESTORED
        }
    }

    pub fn handle_titlebar(&self, _val: bool) {
        warn!("WindowHandle::handle_titlebar is currently unimplemented for gtk.");
    }

    /// Close the window.
    pub fn close(&self) {
        if let Some(state) = self.state.upgrade() {
            state.closing.set(true);
            state.window.close();
        }
    }

    /// Bring this window to the front of the window stack and give it focus.
    pub fn bring_to_front_and_focus(&self) {
        if let Some(state) = self.state.upgrade() {
            // TODO(gtk/misc): replace with present_with_timestamp if/when druid-shell
            // has a system to get the correct input time, as GTK discourages present
            state.window.present();
        }
    }

    /// Request a new paint, but without invalidating anything.
    pub fn request_anim_frame(&self) {
        if let Some(state) = self.state.upgrade() {
            state.request_anim_frame();
        }
    }

    /// Request invalidation of the entire window contents.
    pub fn invalidate(&self) {
        if let Some(state) = self.state.upgrade() {
            self.invalidate_rect(state.area.get().size_dp().to_rect());
        }
    }

    /// Request invalidation of one rectangle, which is given in display points relative to the
    /// drawing area.
    pub fn invalidate_rect(&self, rect: Rect) {
        if let Some(state) = self.state.upgrade() {
            state.invalidate_rect(rect);
        }
    }

    pub fn text(&self) -> PietText {
        PietText::new()
    }

    pub fn request_timer(&self, deadline: Instant) -> TimerToken {
        let interval = deadline
            .checked_duration_since(Instant::now())
            .unwrap_or_default();

        let token = TimerToken::next();

        if let Some(state) = self.state.upgrade() {
            gtk::glib::timeout_add(interval, move || {
                if state.with_handler(|h| h.timer(token)).is_some() {
                    return gtk::glib::Continue(false);
                }
                gtk::glib::Continue(true)
            });
        }
        token
    }

    pub fn set_cursor(&mut self, cursor: &Cursor) {
        if let Some(state) = self.state.upgrade() {
            let cursor = make_gdk_cursor(cursor);
            state.window.set_cursor(cursor.as_ref());
        }
    }

    pub fn make_cursor(&self, desc: &CursorDesc) -> Option<Cursor> {
        if let Some(state) = self.state.upgrade() {
            // TODO: gtk::Pixbuf expects unpremultiplied alpha. We should convert.
            let has_alpha = !matches!(desc.image.format(), ImageFormat::Rgb);
            let bytes_per_pixel = desc.image.format().bytes_per_pixel();
            let pixbuf = Pixbuf::from_mut_slice(
                desc.image.raw_pixels().to_owned(),
                Colorspace::Rgb,
                has_alpha,
                // bits_per_sample
                8,
                desc.image.width() as i32,
                desc.image.height() as i32,
                // row stride (in bytes)
                (desc.image.width() * bytes_per_pixel) as i32,
            );
            let c = gdk::Cursor::from_texture(
                &gdk::Texture::new_for_pixbuf(&pixbuf),
                desc.hot.x.round() as i32,
                desc.hot.y.round() as i32,
                None,
            );
            Some(Cursor::Custom(CustomCursor(c)))
        } else {
            None
        }
    }

    pub fn open_file(&mut self, options: FileDialogOptions) -> Option<FileDialogToken> {
        if let Some(state) = self.state.upgrade() {
            let tok = FileDialogToken::next();
            state.defer(DeferredOp::Open(options, tok));
            Some(tok)
        } else {
            None
        }
    }

    pub fn save_as(&mut self, options: FileDialogOptions) -> Option<FileDialogToken> {
        if let Some(state) = self.state.upgrade() {
            let tok = FileDialogToken::next();
            state.defer(DeferredOp::SaveAs(options, tok));
            Some(tok)
        } else {
            None
        }
    }

    /// Get a handle that can be used to schedule an idle task.
    pub fn get_idle_handle(&self) -> Option<IdleHandle> {
        self.state.upgrade().map(|s| IdleHandle {
            idle_queue: s.idle_queue.clone(),
            state: Arc::downgrade(&s),
        })
    }

    /// Get the `Scale` of the window.
    pub fn get_scale(&self) -> Result<Scale, ShellError> {
        Ok(self
            .state
            .upgrade()
            .ok_or(ShellError::WindowDropped)?
            .scale
            .get())
    }

    pub fn set_menu(&self, menu: Menu) {
        if let Some(state) = self.state.upgrade() {
            let window = &state.window;

            let vbox = window.get_first_child().unwrap()
                .clone()
                .downcast::<gtk::Box>()
                .unwrap();

            let first_child = &vbox.get_first_child().unwrap();
            if first_child.is::<gtk::PopoverMenuBar>() {
                vbox.remove(first_child);
            }
            let menubar = menu.into_gtk_menubar(&self);
            vbox.prepend(&menubar);
        }
    }

    pub fn show_context_menu(&self, menu: Menu, _pos: Point) {
        if let Some(state) = self.state.upgrade() {
            state.defer(DeferredOp::ContextMenu(menu, self.clone()));
        }
    }

    pub fn set_title(&self, title: impl Into<String>) {
        if let Some(state) = self.state.upgrade() {
            state.window.set_title(Some(&*(title.into())));
        }
    }
}

// WindowState needs to be Send + Sync so it can be passed into glib closures.
// TODO: can we localize the unsafety more? Glib's idle loop always runs on the main thread,
// and we always construct the WindowState on the main thread, so it should be ok (and also
// WindowState isn't a public type).
unsafe impl Send for WindowState {}
unsafe impl Sync for WindowState {}

impl IdleHandle {
    /// Add an idle handler, which is called (once) when the message loop
    /// is empty. The idle handler will be run from the main UI thread, and
    /// won't be scheduled if the associated view has been dropped.
    ///
    /// Note: the name "idle" suggests that it will be scheduled with a lower
    /// priority than other UI events, but that's not necessarily the case.
    pub fn add_idle_callback<F>(&self, callback: F)
    where
        F: FnOnce(&dyn Any) + Send + 'static,
    {
        let mut queue = self.idle_queue.lock().unwrap();
        if let Some(state) = self.state.upgrade() {
            if queue.is_empty() {
                queue.push(IdleKind::Callback(Box::new(callback)));
                gtk::glib::idle_add(move || run_idle(&state));
            } else {
                queue.push(IdleKind::Callback(Box::new(callback)));
            }
        }
    }

    pub fn add_idle_token(&self, token: IdleToken) {
        let mut queue = self.idle_queue.lock().unwrap();
        if let Some(state) = self.state.upgrade() {
            if queue.is_empty() {
                queue.push(IdleKind::Token(token));
                gtk::glib::idle_add(move || run_idle(&state));
            } else {
                queue.push(IdleKind::Token(token));
            }
        }
    }
}

fn run_idle(state: &Arc<WindowState>) -> gtk::glib::source::Continue {
    util::assert_main_thread();
    let result = state.with_handler(|handler| {
        let queue: Vec<_> = std::mem::replace(&mut state.idle_queue.lock().unwrap(), Vec::new());

        for item in queue {
            match item {
                IdleKind::Callback(it) => it.call(handler.as_any()),
                IdleKind::Token(it) => handler.idle(it),
            }
        }
    });

    if result.is_none() {
        warn!("Delaying idle callbacks because the handler is borrowed.");
        // Keep trying to reschedule this idle callback, because we haven't had a chance
        // to empty the idle queue. Returning gtk::glib::source::Continue(true) achieves this but
        // causes 100% CPU usage, apparently because glib likes to call us back very quickly.
        let state = Arc::clone(state);
        gtk::glib::timeout_add(std::time::Duration::from_millis(16), move || run_idle(&state));
    }
    gtk::glib::source::Continue(false)
}

fn make_gdk_cursor(cursor: &Cursor) -> Option<gdk::Cursor> {
    if let Cursor::Custom(custom) = cursor {
        Some(custom.0.clone())
    } else {
        gdk::Cursor::from_name(
            match cursor {
                // cursor name values from https://www.w3.org/TR/css-ui-3/#cursor
                Cursor::Arrow => "default",
                Cursor::IBeam => "text",
                Cursor::Crosshair => "crosshair",
                Cursor::OpenHand => "grab",
                Cursor::NotAllowed => "not-allowed",
                Cursor::ResizeLeftRight => "ew-resize",
                Cursor::ResizeUpDown => "ns-resize",
                Cursor::Custom(_) => unreachable!(),
            },
            None,
        )
    }
}

fn get_mouse_button(button: u32) -> Option<MouseButton> {
    match button {
        1 => Some(MouseButton::Left),
        2 => Some(MouseButton::Middle),
        3 => Some(MouseButton::Right),
        // GDK X backend interprets button press events for button 4-7 as scroll events
        8 => Some(MouseButton::X1),
        9 => Some(MouseButton::X2),
        _ => None,
    }
}

fn get_mouse_buttons_from_modifiers(modifiers: gdk::ModifierType) -> MouseButtons {
    let mut buttons = MouseButtons::new();
    if modifiers.contains(ModifierType::BUTTON1_MASK) {
        buttons.insert(MouseButton::Left);
    }
    if modifiers.contains(ModifierType::BUTTON2_MASK) {
        buttons.insert(MouseButton::Middle);
    }
    if modifiers.contains(ModifierType::BUTTON3_MASK) {
        buttons.insert(MouseButton::Right);
    }
    // TODO: Determine X1/X2 state (do caching ourselves if needed)
    //       Checking for BUTTON4_MASK/BUTTON5_MASK does not work with GDK X,
    //       because those are wheel events instead.
    if modifiers.contains(ModifierType::BUTTON4_MASK) {
        buttons.insert(MouseButton::X1);
    }
    if modifiers.contains(ModifierType::BUTTON5_MASK) {
        buttons.insert(MouseButton::X2);
    }
    buttons
}
//replaced
/* fn get_mouse_click_count(event_type: gdk::EventType) -> u8 {
    match event_type {
        gdk::EventType::ButtonPress => 1,
        gdk::EventType::DoubleButtonPress => 2,
        gdk::EventType::TripleButtonPress => 3,
        gdk::EventType::ButtonRelease => 0,
        _ => {
            warn!("Unexpected mouse click event type: {:?}", event_type);
            0
        }
    }
} */

const MODIFIER_MAP: &[(gdk::ModifierType, Modifiers)] = &[
    (ModifierType::SHIFT_MASK, Modifiers::SHIFT),
    (ModifierType::ALT_MASK, Modifiers::ALT),
    (ModifierType::CONTROL_MASK, Modifiers::CONTROL),
    (ModifierType::META_MASK, Modifiers::META),
    (ModifierType::LOCK_MASK, Modifiers::CAPS_LOCK),
    // FIXME: this is the usual value on X11, not sure how consistent it is.
    // Possibly we should use `Keymap::get_num_lock_state()` instead.
    //(ModifierType::MOD2_MASK, Modifiers::NUM_LOCK),
];

fn get_modifiers(modifiers: Option<gdk::ModifierType>) -> Modifiers {
    let mut result = Modifiers::empty();
    if let Some(modi) = modifiers{
        for &(gdk_mod, modifier) in MODIFIER_MAP {
            if modi.contains(gdk_mod) {
                result |= modifier;
            }
        }
    }
    result
}

fn make_key_event(raw_key: &GDKKey, repeat: bool, state: KeyState, modi: Option<ModifierType>) -> KeyEvent {
    let text = raw_key.to_unicode();
    let mods = get_modifiers(modi);
    let key = keycodes::raw_key_to_key(raw_key).unwrap_or_else(|| {
        if let Some(c) = text {
            if c >= ' ' && c != '\x7f' {
                Key::Character(c.to_string())
            } else {
                Key::Unidentified
            }
        } else {
            Key::Unidentified
        }
    });
    let code = keycodes::hardware_keycode_to_code(raw_key);
    let location = keycodes::raw_key_to_location(raw_key);
    let is_composing = false;

    KeyEvent {
        key,
        code,
        location,
        mods,
        repeat,
        is_composing,
        state,
    }
}
