use std::cell::RefCell;
use std::rc::Rc;

use iced::advanced::widget::{Operation, Tree, tree};
use iced::advanced::{Clipboard, Layout, Shell, Widget, layout, mouse, overlay};
use iced::overlay::menu::{self, Menu};
use iced::widget::{button, container, text};
use iced::{Element, Event, Fill, Length, Rectangle, Size, Theme, Vector};

use crate::app::Message;
use crate::app::tabs::{Kind, Message as TabMessage};
use crate::theme;

const OPTIONS: [Kind; 4] = [
    Kind::Library,
    Kind::NowPlaying,
    Kind::SongInformation,
    Kind::Settings,
];

pub(super) fn view() -> Element<'static, Message> {
    let trigger = button(
        container(
            text("+")
                .size(14)
                .line_height(1.0)
                .font(theme::ICON_FONT)
                .align_x(iced::Center)
                .align_y(iced::Center),
        )
        .width(Fill)
        .height(Fill)
        .center_x(Fill)
        .center_y(Fill),
    )
    .width(32)
    .height(32)
    .padding(0)
    .style(theme::window_button_style)
    .on_press(Message::Tabs(TabMessage::DragLeftStrip));

    Element::new(AddTabMenu {
        trigger: trigger.into(),
        menu_class: <Theme as menu::Catalog>::default(),
    })
}

struct AddTabMenu {
    trigger: Element<'static, Message>,
    menu_class: <Theme as menu::Catalog>::Class<'static>,
}

#[derive(Debug, Default)]
struct State {
    open: bool,
    menu: menu::State,
    hovered_option: Option<usize>,
}

impl Widget<Message, Theme, iced::Renderer> for AddTabMenu {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.trigger)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.trigger));
    }

    fn size(&self) -> Size<Length> {
        self.trigger.as_widget().size()
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.trigger
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &iced::Renderer,
        operation: &mut dyn Operation,
    ) {
        self.trigger
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, operation);
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &iced::Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        if matches!(
            event,
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
        ) && cursor.is_over(layout.bounds())
        {
            let state = tree.state.downcast_mut::<State>();
            state.open = !state.open;
            state.hovered_option = None;
            shell.capture_event();
            return;
        }

        self.trigger.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        self.trigger.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
            cursor,
            viewport,
            renderer,
        )
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut iced::Renderer,
        theme: &Theme,
        style: &iced::advanced::renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        self.trigger.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
    }

    fn overlay<'a>(
        &'a mut self,
        tree: &'a mut Tree,
        layout: Layout<'_>,
        _renderer: &iced::Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'a, Message, Theme, iced::Renderer>> {
        let state = tree.state.downcast_mut::<State>();
        if !state.open {
            return None;
        }

        let open = Rc::new(RefCell::new(&mut state.open));
        let selection_open = Rc::clone(&open);
        let menu = Menu::new(
            &mut state.menu,
            &OPTIONS,
            &mut state.hovered_option,
            move |kind| {
                **selection_open.borrow_mut() = false;
                Message::Tabs(TabMessage::Open(kind))
            },
            None,
            &self.menu_class,
        )
        .width(180.0)
        .padding([8, 12])
        .text_size(13);
        let inner = menu.overlay(
            layout.position() + translation,
            *viewport,
            32.0,
            Length::Shrink,
        );

        Some(overlay::Element::new(Box::new(MenuOverlay { inner, open })))
    }
}

struct MenuOverlay<'a> {
    inner: overlay::Element<'a, Message, Theme, iced::Renderer>,
    open: Rc<RefCell<&'a mut bool>>,
}

impl overlay::Overlay<Message, Theme, iced::Renderer> for MenuOverlay<'_> {
    fn layout(&mut self, renderer: &iced::Renderer, bounds: Size) -> layout::Node {
        self.inner.as_overlay_mut().layout(renderer, bounds)
    }

    fn operate(
        &mut self,
        layout: Layout<'_>,
        renderer: &iced::Renderer,
        operation: &mut dyn Operation,
    ) {
        self.inner
            .as_overlay_mut()
            .operate(layout, renderer, operation);
    }

    fn update(
        &mut self,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &iced::Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
    ) {
        let outside = matches!(
            event,
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
        ) && !cursor.is_over(layout.bounds());
        self.inner
            .as_overlay_mut()
            .update(event, layout, cursor, renderer, clipboard, shell);
        if outside {
            **self.open.borrow_mut() = false;
            shell.capture_event();
        }
    }

    fn mouse_interaction(
        &self,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        self.inner
            .as_overlay()
            .mouse_interaction(layout, cursor, renderer)
    }

    fn draw(
        &self,
        renderer: &mut iced::Renderer,
        theme: &Theme,
        style: &iced::advanced::renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
    ) {
        self.inner
            .as_overlay()
            .draw(renderer, theme, style, layout, cursor);
    }

    fn overlay<'a>(
        &'a mut self,
        layout: Layout<'a>,
        renderer: &iced::Renderer,
    ) -> Option<overlay::Element<'a, Message, Theme, iced::Renderer>> {
        self.inner.as_overlay_mut().overlay(layout, renderer)
    }
}
