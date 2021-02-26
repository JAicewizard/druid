// Copyright 2020 The Druid Authors.
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

//! GTK Monitors and Screen information.

use crate::screen::Monitor;
use gdk::Display;
use kurbo::{Point, Rect, Size};
use gtk::gio::{ListModelExt, ListModel};
use gtk::glib::object::Cast;

fn translate_gdk_rectangle(r: gdk::Rectangle) -> Rect {
    Rect::from_origin_size(
        Point::new(r.x as f64, r.y as f64),
        Size::new(r.width as f64, r.height as f64),
    )
}

fn translate_gdk_monitor(mon: gdk::Monitor, is_default: bool) -> Monitor {
    let area = translate_gdk_rectangle(mon.get_geometry());
    Monitor::new(
        is_default,
        area,
        translate_gdk_rectangle(mon.get_geometry())
    )
}

pub(crate) fn get_monitors() -> Vec<Monitor> {

    let display = gdk::Display::get_default().unwrap();
let defailt_monitors: &Vec<gdk::Monitor> = &display.get_monitors().map(|display: ListModel| {
    (0..display.get_n_items())
        .map(move |i| display.get_object(i).unwrap().downcast::<gdk::Monitor>().unwrap())
}).unwrap().collect();

    gdk::DisplayManager::get().unwrap()
    .list_displays()
    .iter()
    .flat_map( |display: &Display| {
        display.get_monitors()
        .map(move |display: ListModel| {
            (0..display.get_n_items())
                .map(move |i| translate_gdk_monitor(display.get_object(i).unwrap().downcast::<gdk::Monitor>().unwrap(), defailt_monitors.contains(&display.get_object(i).unwrap().downcast::<gdk::Monitor>().unwrap())))
        }).unwrap()
        .collect::<Vec<Monitor>>()
    }).collect::<Vec<Monitor>>()



}
