use iced::advanced::widget::{Operation, Tree, tree};
use iced::advanced::{Clipboard, Layout, Shell, Widget, layout, mouse, overlay, renderer};
use iced::widget::{button, container, text, vertical_slider};
use iced::{Center, Element, Event, Fill, Point, Rectangle, Size, Theme, Vector};

use super::PLAYBACK_BAR_HEIGHT;
use crate::app::Message;
use crate::theme;

const BUTTON_SIZE: f32 = PLAYBACK_BAR_HEIGHT - 2.0 * theme::CHROME_BORDER_WIDTH;
const POPUP_WIDTH: f32 = BUTTON_SIZE;
const POPUP_HEIGHT: f32 = 120.0;
const POPUP_GAP: f32 = 8.0;
const SLIDER_WIDTH: f32 = 16.0;

pub(super) fn view(volume: u8) -> Element<'static, Message> {
    let volume = volume.min(100);
    let label = format!("{volume}");

    let trigger = button(
        container(
            iced::widget::column![
                text("VOL")
                    .size(9)
                    .line_height(1.0)
                    .font(theme::ICON_FONT)
                    .align_x(Center),
                text(label)
                    .size(13)
                    .line_height(1.0)
                    .font(theme::STRONG_FONT)
                    .align_x(Center),
            ]
            .spacing(3)
            .align_x(Center),
        )
        .width(Fill)
        .height(Fill)
        .center_x(Fill)
        .center_y(Fill),
    )
    .width(BUTTON_SIZE)
    .height(BUTTON_SIZE)
    .padding(0)
    .style(theme::transport_button_style)
    .on_press(Message::VolumeChanged(volume));

    let popup = container(
        vertical_slider(0..=100, volume, Message::VolumeChanged)
            .step(1u8)
            .width(SLIDER_WIDTH)
            .height(Fill)
            .style(theme::volume_slider_style),
    )
    .width(POPUP_WIDTH)
    .height(POPUP_HEIGHT)
    .padding([14, 0])
    .center_x(Fill)
    .style(theme::volume_popup_style);

    Element::new(VolumeControl {
        trigger: trigger.into(),
        popup: popup.into(),
    })
}

struct VolumeControl {
    trigger: Element<'static, Message>,
    popup: Element<'static, Message>,
}

#[derive(Debug, Default)]
struct State {
    open: bool,
}

impl Widget<Message, Theme, iced::Renderer> for VolumeControl {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.trigger), Tree::new(&self.popup)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(&[&self.trigger, &self.popup]);
    }

    fn size(&self) -> Size<iced::Length> {
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
        let state = tree.state.downcast_mut::<State>();
        let over_button = cursor.is_over(layout.bounds());

        if matches!(
            event,
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
        ) {
            if over_button {
                state.open = !state.open;
                shell.capture_event();
                shell.invalidate_layout();
                return;
            }

            if state.open {
                // Click fell through the overlay (outside popup + button) → close.
                state.open = false;
                shell.invalidate_layout();
            }
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
        style: &renderer::Style,
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
        _viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'a, Message, Theme, iced::Renderer>> {
        let state = tree.state.downcast_mut::<State>();
        if !state.open {
            return None;
        }

        let open = &mut state.open;
        let popup_tree = tree.children.get_mut(1)?;

        Some(overlay::Element::new(Box::new(VolumeOverlay {
            position: layout.position() + translation,
            button_bounds: layout.bounds() + translation,
            popup: &mut self.popup,
            tree: popup_tree,
            open,
        })))
    }
}

struct VolumeOverlay<'a> {
    position: Point,
    button_bounds: Rectangle,
    popup: &'a mut Element<'static, Message>,
    tree: &'a mut Tree,
    open: &'a mut bool,
}

impl overlay::Overlay<Message, Theme, iced::Renderer> for VolumeOverlay<'_> {
    fn layout(&mut self, renderer: &iced::Renderer, bounds: Size) -> layout::Node {
        let popup_layout = self.popup.as_widget_mut().layout(
            self.tree,
            renderer,
            &layout::Limits::new(Size::ZERO, bounds).width(POPUP_WIDTH).height(POPUP_HEIGHT),
        );

        let size = popup_layout.size();
        let x = self.position.x + (self.button_bounds.width - size.width) / 2.0;
        let y = self.position.y - size.height - POPUP_GAP;

        layout::Node::with_children(size, vec![popup_layout])
            .translate(Vector::new(x, y))
    }

    fn operate(
        &mut self,
        layout: Layout<'_>,
        renderer: &iced::Renderer,
        operation: &mut dyn Operation,
    ) {
        let Some(child) = layout.children().next() else {
            return;
        };
        self.popup
            .as_widget_mut()
            .operate(self.tree, child, renderer, operation);
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
        let bounds = layout.bounds();
        let over_popup = cursor.is_over(bounds);
        let over_button = cursor.is_over(self.button_bounds);

        if matches!(
            event,
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
        ) && !over_popup
            && !over_button
        {
            *self.open = false;
            shell.capture_event();
            shell.invalidate_layout();
            return;
        }

        let Some(child) = layout.children().next() else {
            return;
        };

        self.popup.as_widget_mut().update(
            self.tree,
            event,
            child,
            cursor,
            renderer,
            clipboard,
            shell,
            &Rectangle::with_size(Size::INFINITE),
        );
    }

    fn mouse_interaction(
        &self,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        let Some(child) = layout.children().next() else {
            return mouse::Interaction::None;
        };

        self.popup.as_widget().mouse_interaction(
            self.tree,
            child,
            cursor,
            &Rectangle::with_size(Size::INFINITE),
            renderer,
        )
    }

    fn draw(
        &self,
        renderer: &mut iced::Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
    ) {
        let Some(child) = layout.children().next() else {
            return;
        };

        self.popup.as_widget().draw(
            self.tree,
            renderer,
            theme,
            style,
            child,
            cursor,
            &Rectangle::with_size(Size::INFINITE),
        );
    }
}
