use cushy::{
    figures::Zero,
    styles::{CornerRadii, Styles, components::CornerRadius},
    widget::{MakeWidget, MakeWidgetWithTag, WidgetTag},
    widgets::{
        Label, Menu,
        layers::{OverlayLayer, Overlayable},
        menu::MenuItem,
    },
};

fn menu_button<T: Options + 'static>(
    label: &'static str,
    window_overlay: OverlayLayer,
    on_selected: impl FnMut(T) + Send + 'static,
) -> impl MakeWidget {
    let mut menu = Menu::new().on_selected(on_selected);
    for item in T::enumerate() {
        let label = item.label();
        menu = menu.with(MenuItem::new(item.clone(), Label::new(label)));
    }
    let (wtag, wid) = WidgetTag::new();
    let btn = label.into_button();
    let show_menu = {
        let overlay = window_overlay.clone();
        move |ev: Option<cushy::widgets::button::ButtonClick>| {
            if let Some(ev) = ev {
                menu.overlay_in(&overlay)
                    .hide_on_unhover()
                    .at(ev.window_location)
                    // .below(wid)
                    .show();
            }
        }
    };
    btn.on_click(show_menu)
        .with_styles(Styles::new().with(&CornerRadius, CornerRadii::ZERO))
        .make_with_tag(wtag)
}

trait Options: Unpin + Clone + Send + Sync + std::fmt::Debug {
    fn label(&self) -> String;
    fn enumerate() -> &'static [Self];
}

macro_rules! menu_options {
($($pub:vis enum $name:ident {$($var:ident),* $(,)?})*) => {
    $(

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    $pub enum $name {
        $($var),*
    }

    impl $name {
        pub fn label(self) -> &'static str {
            match self {
                $(Self::$var => stringify!($var)),*
            }
        }
    }

    impl Options for $name {
        fn label(&self) -> String {
            <$name>::label(*self).to_string()
        }
        fn enumerate() -> &'static [Self] {
            &[
                $(Self::$var),*
            ]
        }
    }

    )*
};
}

menu_options! {
    pub enum FileOptions {
        Open,
    }
}
