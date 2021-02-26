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

//! Interactions with the system pasteboard on GTK+.

use gdk::{ContentProvider,ContentProviderExt, Display,ContentFormats};
use gtk::glib::value::Value;
use gtk::glib::types::Type;
use gtk::glib::Bytes;
use gtk::glib::GString;
use gtk::glib::source::PRIORITY_HIGH;
use gtk::glib::Error;
use gtk::gio::prelude::InputStreamExt;
use gtk::gio::InputStream;
use gtk::gio::NONE_CANCELLABLE;
use std::sync::mpsc::{Sender, Receiver};
use std::sync::mpsc;

use crate::clipboard::{ClipboardFormat, FormatId};

use core::convert::AsRef;

/// The system clipboard.
#[derive(Debug, Clone)]
pub struct Clipboard;

impl Clipboard {
    /// Put a string onto the system clipboard.
    pub fn put_string(&mut self, s: impl AsRef<str>) {
        let display = Display::get_default().unwrap();
        let clipboard = display.get_clipboard();

        clipboard.set_text(s.as_ref())
    }

    /// Put multi-format data on the system clipboard.
    pub fn put_formats(&mut self, formats: &[ClipboardFormat]) {
        let display = Display::get_default().unwrap();
        let clipboard = display.get_clipboard();

        let mut providers = Vec::<ContentProvider>::new();
        for format in formats{
            providers.push(ContentProvider::new_for_bytes(format.identifier, &Bytes::from_owned(format.data.clone())))
        }

        let provider = ContentProvider::new_union(&*providers);
        if !clipboard.set_content(Some(&provider)){
            tracing::warn!("failed to set clipboard data.");

        }
    }

    /// Get a string from the system clipboard, if one is available.
    pub fn get_string(&self) -> Option<String> {
        let display = Display::get_default().unwrap();
        let clipboard = display.get_clipboard();
        let provider = clipboard.get_content()?;

        let mut value = Value::from_type(Type::String);

        provider.get_value(&mut value).ok()?;
        if let Ok(string) = value.get_some::<Vec<String>>(){
            string.last().map(|x|x.clone())
        }else{
            None
        }
    }

    /// Given a list of supported clipboard types, returns the supported type which has
    /// highest priority on the system clipboard, or `None` if no types are supported.
    pub fn preferred_format(&self, formats: &[FormatId]) -> Option<FormatId> {
        let display = gdk::Display::get_default().unwrap();
        let clipboard = display.get_clipboard();
        let targets = clipboard.get_formats()?;
        for format in formats {
            if targets.contain_mime_type(format){
                return Some(format)
            }
        }
        None
    }

    /// Return data in a given format, if available.
    ///
    /// It is recommended that the `fmt` argument be a format returned by
    /// [`Clipboard::preferred_format`]
    pub fn get_format(&self, format: FormatId) -> Option<Vec<u8>> {
        //TODO: COMPLETELY UNTESTED PLS TEST
        let display = Display::get_default().unwrap();
        let clipboard = display.get_clipboard();

        let (tx, rx): (Sender<Option::<Vec<u8>>>, Receiver<Option::<Vec<u8>>>) = mpsc::channel();
        clipboard.read_async(&[format],PRIORITY_HIGH, NONE_CANCELLABLE, move |clip_data: Result<(InputStream, GString), Error>|{
            if clip_data.is_ok(){
                let bytes = (clip_data.ok().unwrap()).0.read_bytes(usize::MAX, NONE_CANCELLABLE);
                if bytes.is_ok(){
                    tx.send(Some(Vec::from(AsRef::<[u8]>::as_ref(&bytes.unwrap())))).unwrap();
                }else{
                    tx.send(None).unwrap();
                }
            }else{
                tx.send(None).unwrap();
            }
        });
        rx.recv().unwrap()
    }

    pub fn available_type_names(&self) -> Vec<String> {
        let display = gdk::Display::get_default().unwrap();
        let clipboard = display.get_clipboard();
        let formats = clipboard.get_formats().unwrap_or_else(||ContentFormats::new(&[]));
        formats.get_mime_types().0.iter().map(|s|String::from(s.as_str())).collect()
    }
}
