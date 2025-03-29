use super::*;
#[derive(Clone, Debug, PartialEq)]
pub struct Game {
    pub name: Dynamic<String>,
    pub id: Dynamic<String>,
    pub cover: Dynamic<AnyTexture>,
}

impl Game {
    pub fn new(
        name: impl Into<String>,
        id: impl Into<String>,
        cover: impl Into<AnyTexture>,
    ) -> Self {
        Self {
            name: Dynamic::new(name.into()),
            id: Dynamic::new(id.into()),
            cover: Dynamic::new(cover.into()),
        }
    }
}

impl MakeWidget for Game {
    fn make_widget(self) -> cushy::widget::WidgetInstance {
        let img_size = Lp::new(140);
        let title = self.name.to_input().and(self.id.to_label()).into_columns();
        Image::new(self.cover)
            .aspect_fit()
            .size(Size::new(img_size, img_size))
            .and(title.and("Description".to_label()).into_rows())
            .into_columns()
            .make_widget()
    }
}
