use super::*;

#[easy_ext::ext]
pub impl<T: Send + 'static> Sender<T> {
    fn update(&self, value: T) {
        self.blocking_send(value)
            .expect("Failed to send message through update channel")
    }
}

#[easy_ext::ext]
pub impl<T: Send + Clone + 'static, D> D
where
    D: Source<T>,
{
    fn connect_const<U: Send + 'static + Clone>(&self, tx: &Sender<U>, value: U) {
        self.on_change(closure!([tx] () tx.update(value.clone())))
            .persist();
    }
    fn connect<U: Send + 'static>(&self, tx: &Sender<U>, map: impl Fn(T) -> U + Send + 'static) {
        self.for_each_cloned(closure!([tx] (val) tx.update(map(val))))
            .persist();
    }
}

macro_rules! binds {
    ($bind:ident = $expr:expr $(, $($tt:tt)*)?) => {
        let $bind = $expr;
        binds!($($($tt)*)?)
    };
    ($ident:ident as $bind:ident $(, $($tt:tt)*)?) => {
        let $bind = $ident.clone();
        binds!($($($tt)*)?)
    };
    ($ident:ident $(, $($tt:tt)*)?) => {
        let $ident = $ident.clone();
        binds!($($($tt)*)?)
    };
    (mut $ident:ident $(, $($tt:tt)*)?) => {
        let mut $ident = $ident.clone();
        binds!($($($tt)*)?)
    };
    (&mut $ident:ident $(, $($tt:tt)*)?) => {
        let mut $ident = $ident;
        binds!($($($tt)*)?)
    };
    (&$ident:ident $(, $($tt:tt)*)?) => {
        let $ident = $ident;
        binds!($($($tt)*)?)
    };
    ($(,)?) => { }
}

macro_rules! closure {
    ([$($tt:tt)*] ($($arg:pat),*) $expr:expr) => {{
        binds!($($tt)*);
        move |$($arg),*| $expr
    }};
    (async [$($tt:tt)*] $expr:expr) => {{
        binds!($($tt)*);
        async move {$expr}
    }};
}
pub(crate) use binds;
pub(crate) use closure;
