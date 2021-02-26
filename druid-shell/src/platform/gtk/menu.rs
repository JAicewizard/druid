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

//! GTK implementation of menus.

use gdk::ModifierType;
use gtk::{  WidgetExt};
use gtk::{PopoverMenu,PopoverMenuBar};
use gtk::ButtonExt;
use gtk::gio::Menu as GIOMenu;
use super::keycodes;
use super::window::WindowHandle;
use crate::common_util::strip_access_key;
use crate::hotkey::{HotKey, RawMods};
use crate::keyboard::{KbKey, Modifiers};

#[derive(Default, Debug, Clone)]
pub struct Menu {
    items: Vec<MenuItem>,
}

#[derive(Debug, Clone)]
enum MenuItem {
    Entry {
        name: String,
        id: u32,
        key: Option<HotKey>,
        enabled: bool,
    },
    SubMenu(String, Menu),
    Separator,
}

impl Menu {
    pub fn new() -> Menu {
        Menu { items: Vec::new() }
    }

    pub fn new_for_popup() -> Menu {
        Menu { items: Vec::new() }
    }

    pub fn add_dropdown(&mut self, menu: Menu, text: &str, _enabled: bool) {
        // TODO: implement enabled dropdown
        self.items
            .push(MenuItem::SubMenu(strip_access_key(text), menu));
    }

    pub fn add_item(
        &mut self,
        id: u32,
        text: &str,
        key: Option<&HotKey>,
        enabled: bool,
        _selected: bool,
    ) {
        // TODO: implement selected items
        self.items.push(MenuItem::Entry {
            name: strip_access_key(text),
            id,
            key: key.cloned(),
            enabled,
        });
    }

    pub fn add_separator(&mut self) {
        self.items.push(MenuItem::Separator)
    }

    fn append_items_to_menu(
        self,
        menu: &mut PopoverMenuBar,
        handle: &WindowHandle,
    ) {
        let mut i = 0;

        for item in self.items {
            match item {
                MenuItem::Entry {
                    name,
                    id,
                    key,
                    enabled,
                } => {
                    let item = gtk::Button::with_label(&name);
                    item.set_sensitive(enabled);

                    if let Some(k) = key {
                        let controller = gtk::ShortcutController::new();
                        controller.add_shortcut(&register_accelerator(&k));
                        item.add_controller(&controller)
                    }

                    let handle = handle.clone();
                    item.connect_activate(move |_| {
                        if let Some(state) = handle.state.upgrade() {
                            state.handler.borrow_mut().command(id);
                        }
                    });

                    menu.add_child(&item,name.as_str());
                }
                MenuItem::SubMenu(name, submenu) => {
                    let item = gtk::MenuButton::new();
                    item.set_label(&name);
                    item.set_popover(Some(&submenu.clone().into_gtk_menu(handle)));

                    menu.add_child(&item,name.as_str());
                }
                MenuItem::Separator => {
                    i+=1;
                    menu.add_child(&gtk::Separator::new(gtk::Orientation::Horizontal),format!("sep{}",i).as_str());
                },
            }
        }
    }
    fn append_items_to_menu_nonbar(
        self,
        menu: &mut PopoverMenu,
        handle: &WindowHandle,
    ) {
        let mut i = 0;

        for item in self.items {
            match item {
                MenuItem::Entry {
                    name,
                    id,
                    key,
                    enabled,
                } => {
                    let item = gtk::Button::with_label(&name);
                    item.set_sensitive(enabled);

                    if let Some(k) = key {
                        let controller = gtk::ShortcutController::new();
                        controller.add_shortcut(&register_accelerator(&k));
                        item.add_controller(&controller)
                    }

                    let handle = handle.clone();
                    item.connect_activate(move |_| {
                        if let Some(state) = handle.state.upgrade() {
                            state.handler.borrow_mut().command(id);
                        }
                    });

                    menu.add_child(&item,name.as_str());
                }
                MenuItem::SubMenu(name, submenu) => {
                    let item = gtk::MenuButton::new();
                    item.set_label(&name);
                    item.set_popover(Some(&submenu.clone().into_gtk_menu(handle)));

                    menu.add_child(&item,name.as_str());
                }
                MenuItem::Separator => {
                    i+=1;
                    menu.add_child(&gtk::Separator::new(gtk::Orientation::Horizontal),format!("sep{}",i).as_str());
                },
            }
        }
    }

    fn append_items_to_giomenu(
        self,
        menu: &mut GIOMenu,
    ) {
        let mut i = 0;
        for item in &self.items {
            match item {
                MenuItem::Entry {
                    name,
                    id,
                    key,
                    enabled,
                } => {
                    menu.append(Some(name.as_str()), None);
                }
                MenuItem::SubMenu(name, submenu) => {
                    let mut item = GIOMenu::new();
                    Some(&submenu.clone().append_items_to_giomenu(&mut item));
                    menu.append_submenu(Some(name.as_str()),&item);
                }
                MenuItem::Separator => {
                    i+=1;
                    menu.append(Some(format!("sep{}",i).as_str()), None)
                },
            }
        }
    }

    pub(crate) fn into_gtk_menubar(
        self,
        handle: &WindowHandle,
    ) -> PopoverMenuBar {
        let mut gio_menu = GIOMenu::new();
        self.clone().append_items_to_giomenu(&mut gio_menu);

        let mut menu = PopoverMenuBar::from_model(Some(&gio_menu));

        self.append_items_to_menu(&mut menu, handle);

        menu
    }

    pub fn into_gtk_menu(self, handle: &WindowHandle) -> PopoverMenu {
        let mut gio_menu = GIOMenu::new();
        self.clone().append_items_to_giomenu(&mut gio_menu);
        let mut menu = PopoverMenu::from_model(Some(&gio_menu));

        self.append_items_to_menu_nonbar(&mut menu, handle);

        menu
    }
}

fn register_accelerator(menu_key: &HotKey) -> gtk::Shortcut {
    let gdk_keyval = match &menu_key.key {
        KbKey::Character(text) => text.chars().next().unwrap(),
        k => {
            if let Some(gdk_key) = keycodes::key_to_raw_key(k) {
                gdk_key.to_unicode().unwrap()
            } else {
                tracing::warn!("Cannot map key {:?}", k);
                return gtk::Shortcut::new::<gtk::ShortcutTrigger,gtk::ActivateAction >(None, None);
            }
        }
    };
    let trig = gtk::ShortcutTrigger::parse_string(format!("{}{}",modifiers_to_gdk_modifier_string(menu_key.mods),gdk_keyval).as_str()).unwrap();
    let action = gtk::ActivateAction::get().unwrap();

    gtk::Shortcut::new::<gtk::ShortcutTrigger,gtk::ActivateAction >(Some(&trig), Some(&action))
}

fn modifiers_to_gdk_modifier_type(raw_modifiers: RawMods) -> gdk::ModifierType {
    let mut result = ModifierType::empty();

    let modifiers: Modifiers = raw_modifiers.into();

    result.set(ModifierType::ALT_MASK, modifiers.alt());
    result.set(ModifierType::CONTROL_MASK, modifiers.ctrl());
    result.set(ModifierType::SHIFT_MASK, modifiers.shift());
    result.set(ModifierType::META_MASK, modifiers.meta());

    result
}

fn modifiers_to_gdk_modifier_string(raw_modifiers: RawMods) -> String {
    let mut result = String::from("");

    let modifiers: Modifiers = raw_modifiers.into();

    if modifiers.alt(){
        result = format!("{}{}", result, "<Alt>")
    }
    if modifiers.ctrl(){
        result = format!("{}{}", result, "<Control>")
    }
    if modifiers.shift(){
        result = format!("{}{}", result, "<Shift>")
    }
    if modifiers.meta(){
        result = format!("{}{}", result, "<Meta>")
    }

    result
}
