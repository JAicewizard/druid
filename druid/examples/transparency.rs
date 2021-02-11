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

//! An example of a transparent window background.
//! Useful for dropdowns, tooltips and other overlay windows.

use druid::{kurbo::{Circle}, widget::{Button, Flex}};
use druid::widget::prelude::*;
use druid::{
    AppLauncher, Color, LocalizedString, Rect,
    WindowDesc,
};

struct CustomWidget;

impl Widget<String> for CustomWidget {
    fn event(&mut self, _ctx: &mut EventCtx, _event: &Event, _data: &mut String, _env: &Env) {}

    fn lifecycle(
        &mut self,
        _ctx: &mut LifeCycleCtx,
        _event: &LifeCycle,
        _data: &String,
        _env: &Env,
    ) {
    }

    fn update(&mut self, _ctx: &mut UpdateCtx, _old_data: &String, _data: &String, _env: &Env) {}

    fn layout(
        &mut self,
        _layout_ctx: &mut LayoutCtx,
        bc: &BoxConstraints,
        _data: &String,
        _env: &Env,
    ) -> Size {

        if bc.is_width_bounded() | bc.is_height_bounded() {
            let size = Size::new(100.0, 100.0);
            bc.constrain(size)
        } else {
            bc.max()
        }
    }

    // The paint method gets called last, after an event flow.
    // It goes event -> update -> layout -> paint, and each method can influence the next.
    // Basically, anything that changes the appearance of a widget causes a paint.
    fn paint(&mut self, ctx: &mut PaintCtx, data: &String, env: &Env) {
        ctx.clear(Color::rgba8(0x0,0x0,0x0,0x0));
        // let size = ctx.size();
        // let rect = size.to_rect();
        // ctx.fill(rect, &Color::rgba8(0x0,0x0,0x0,0x0));

        let circle = Circle::new((130., 130.), 100.);
        ctx.fill(circle, &Color::RED);

        let circle = Rect::new(100., 100., 300., 300.);
        ctx.fill(circle, &Color::rgba8(0x0, 0x0, 0xff, 125));

    }
}

pub fn main() {
    let btn = Button::new("Hello there");
    let example = Flex::column().with_child(CustomWidget {}).with_child(btn);
    let window = WindowDesc::new(example)
        .show_titlebar(false)
        .window_size((823., 823.))
        .transparent(true)
        .resizable(true)
        .title(LocalizedString::new("Fancy Colors"));
    
    AppLauncher::with_window(window)
        .use_simple_logger()
        .launch("Druid + Piet".to_string())
        .expect("launch failed");
}
