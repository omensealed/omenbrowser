use iced::advanced::{
    layout, mouse, overlay, renderer,
    widget::{Operation, Tree},
    Clipboard, Layout, Shell, Widget,
};
use iced::{Element, Event, Length, Point, Rectangle, Size, Vector};

/// Places interactive actions over an underlay without contributing to its
/// layout. The action surface remains open while the pointer moves between the
/// underlay and the floating controls.
pub(in crate::desktop) struct HoverActions<'a, Message> {
    underlay: Element<'a, Message>,
    actions: Option<Element<'a, Message>>,
}

impl<'a, Message> HoverActions<'a, Message> {
    pub(in crate::desktop) fn new(
        underlay: impl Into<Element<'a, Message>>,
        actions: Option<impl Into<Element<'a, Message>>>,
    ) -> Self {
        Self {
            underlay: underlay.into(),
            actions: actions.map(Into::into),
        }
    }
}

impl<'a, Message> Widget<Message, iced::Theme, iced::Renderer> for HoverActions<'a, Message>
where
    Message: 'a,
{
    fn size(&self) -> Size<Length> {
        self.underlay.as_widget().size()
    }

    fn size_hint(&self) -> Size<Length> {
        self.underlay.as_widget().size_hint()
    }

    fn children(&self) -> Vec<Tree> {
        vec![
            Tree::new(&self.underlay),
            self.actions.as_ref().map_or_else(Tree::empty, Tree::new),
        ]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.children[0].diff(&self.underlay);
        if let Some(actions) = self.actions.as_ref() {
            tree.children[1].diff(actions);
        } else {
            tree.children[1] = Tree::empty();
        }
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.underlay
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut iced::Renderer,
        theme: &iced::Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        self.underlay.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
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
        self.underlay.as_widget_mut().update(
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
        self.underlay.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
            cursor,
            viewport,
            renderer,
        )
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &iced::Renderer,
        operation: &mut dyn Operation,
    ) {
        self.underlay
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, operation);
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &iced::Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, iced::Theme, iced::Renderer>> {
        let (underlay_tree, action_tree) = tree.children.split_at_mut(1);
        let underlay_overlay = self.underlay.as_widget_mut().overlay(
            &mut underlay_tree[0],
            layout,
            renderer,
            viewport,
            translation,
        );
        let action_overlay = self.actions.as_mut().map(|actions| {
            overlay::Element::new(Box::new(ActionOverlay {
                anchor: layout.bounds() + translation,
                content: actions,
                tree: &mut action_tree[0],
            }))
        });
        let children = underlay_overlay
            .into_iter()
            .chain(action_overlay)
            .collect::<Vec<_>>();
        (!children.is_empty()).then(|| overlay::Group::with_children(children).overlay())
    }
}

impl<'a, Message> From<HoverActions<'a, Message>> for Element<'a, Message>
where
    Message: 'a,
{
    fn from(actions: HoverActions<'a, Message>) -> Self {
        Element::new(actions)
    }
}

struct ActionOverlay<'a, 'b, Message> {
    anchor: Rectangle,
    content: &'a mut Element<'b, Message>,
    tree: &'a mut Tree,
}

fn anchored_action_position(anchor: Rectangle, content: Size, bounds: Size) -> Point {
    Point::new(
        (anchor.x + anchor.width - content.width)
            .clamp(0.0, (bounds.width - content.width).max(0.0)),
        anchor
            .y
            .clamp(0.0, (bounds.height - content.height).max(0.0)),
    )
}

impl<Message> overlay::Overlay<Message, iced::Theme, iced::Renderer>
    for ActionOverlay<'_, '_, Message>
{
    fn layout(&mut self, renderer: &iced::Renderer, bounds: Size) -> layout::Node {
        let limits = layout::Limits::new(Size::ZERO, bounds);
        let mut content = self
            .content
            .as_widget_mut()
            .layout(self.tree, renderer, &limits);
        content.move_to_mut(anchored_action_position(
            self.anchor,
            content.size(),
            bounds,
        ));
        layout::Node::with_children(bounds, vec![content])
    }

    fn draw(
        &self,
        renderer: &mut iced::Renderer,
        theme: &iced::Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
    ) {
        if let Some(content_layout) = layout.children().next() {
            self.content.as_widget().draw(
                self.tree,
                renderer,
                theme,
                style,
                content_layout,
                cursor,
                &layout.bounds(),
            );
        }
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
        let Some(content_layout) = layout.children().next() else {
            return;
        };
        self.content.as_widget_mut().update(
            self.tree,
            event,
            content_layout,
            cursor,
            renderer,
            clipboard,
            shell,
            &layout.bounds(),
        );
        if cursor.is_over(content_layout.bounds())
            && matches!(event, Event::Mouse(_) | Event::Touch(_))
        {
            shell.capture_event();
        }
    }

    fn mouse_interaction(
        &self,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        layout
            .children()
            .next()
            .map_or(mouse::Interaction::None, |content_layout| {
                self.content.as_widget().mouse_interaction(
                    self.tree,
                    content_layout,
                    cursor,
                    &layout.bounds(),
                    renderer,
                )
            })
    }

    fn operate(
        &mut self,
        layout: Layout<'_>,
        renderer: &iced::Renderer,
        operation: &mut dyn Operation,
    ) {
        if let Some(content_layout) = layout.children().next() {
            self.content
                .as_widget_mut()
                .operate(self.tree, content_layout, renderer, operation);
        }
    }

    fn overlay<'c>(
        &'c mut self,
        layout: Layout<'c>,
        renderer: &iced::Renderer,
    ) -> Option<overlay::Element<'c, Message, iced::Theme, iced::Renderer>> {
        let content_layout = layout.children().next()?;
        self.content.as_widget_mut().overlay(
            self.tree,
            content_layout,
            renderer,
            &layout.bounds(),
            Vector::ZERO,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::anchored_action_position;
    use iced::{Point, Rectangle, Size};

    #[test]
    fn action_surface_anchors_right_without_changing_underlay_geometry() {
        let anchor = Rectangle::new(Point::new(20.0, 30.0), Size::new(300.0, 16.0));
        assert_eq!(
            anchored_action_position(anchor, Size::new(120.0, 37.0), Size::new(500.0, 400.0)),
            Point::new(200.0, 30.0)
        );
    }

    #[test]
    fn action_surface_stays_inside_the_viewport() {
        let anchor = Rectangle::new(Point::new(5.0, 390.0), Size::new(40.0, 16.0));
        assert_eq!(
            anchored_action_position(anchor, Size::new(120.0, 37.0), Size::new(100.0, 400.0)),
            Point::new(0.0, 363.0)
        );
    }
}
