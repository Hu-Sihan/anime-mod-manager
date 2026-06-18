/// Wrapper around rust_i18n's `t!` macro — always returns `String`.
/// Works with GTK builders (impl Into<GString>), format!(), and String contexts.
/// Supports positional `{}` interpolation for optional arguments.
#[macro_export]
macro_rules! tr {
    ($key:expr $(, $args:expr)* $(,)?) => {{
        let _tr = ::rust_i18n::t!($key).to_string();
        #[allow(unused_mut)]
        let mut _result = String::new();
        let mut _pos: usize = 0;
        let _len = _tr.len();
        $({
            if let Some(_found) = _tr[_pos..].find("{}") {
                _result.push_str(&_tr[_pos.._pos + _found]);
                use ::std::fmt::Write as _;
                let _ = write!(_result, "{}", $args);
                _pos = _pos + _found + 2;
            } else {
                _result.push_str(&_tr[_pos..]);
                _pos = _len;
            }
        })*
        if _pos < _len {
            _result.push_str(&_tr[_pos..]);
        }
        _result
    }};
}

pub fn switch_language(locale: &str) {
    rust_i18n::set_locale(locale);
}
