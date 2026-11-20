// Compile-time, first-match selection for items and statements. Arm bodies must
// be braced; expression invocations use cfg_select! {{ ... }} to provide a scope.
// At MSRV 1.95, switch to core::cfg_select! and drop the extra expression braces.
#[allow(
    unused_macros,
    reason = "The private helper is available regardless of which optional consumers are enabled."
)]
macro_rules! cfg_select {
    ({ $($arms:tt)* }) => {{
        cfg_select! { $($arms)* }
    }};
    (_ => { $($code:tt)* } $(,)?) => {
        $($code)*
    };
    ($condition:meta => { $($code:tt)* }, $($rest:tt)*) => {
        cfg_select! { $condition => { $($code)* } $($rest)* }
    };
    ($condition:meta => { $($code:tt)* } $($rest:tt)*) => {
        #[cfg($condition)]
        cfg_select! { _ => { $($code)* } }
        #[cfg(not($condition))]
        cfg_select! { $($rest)* }
    };
    () => {
        compile_error!("cfg_select!: no configuration predicate matched");
    };
}

#[cfg(test)]
mod tests {
    cfg_select! {
        any() => { invalid Rust tokens }
        all() => {
            type Selected = u8;
            const VALUE: Selected = 42;
        }
        all() => { invalid Rust tokens }
        _ => { invalid Rust tokens }
    }

    cfg_select! {
        any() => { invalid Rust tokens },
        _ => {
            type Fallback = u16;
            const FALLBACK: Fallback = 7;
        },
    }

    #[test]
    fn selects_only_the_first_matching_items() {
        assert_eq!(VALUE, 42);
        assert_eq!(FALLBACK, 7);
    }

    #[test]
    fn expressions_support_const_context_and_optional_fallback() {
        const SELECTED: u8 = cfg_select! {{
            any() => { undefined_value() },
            all() => { VALUE },
            all() => { "wrong type" },
        }};
        assert_eq!(SELECTED, 42);
    }

    #[test]
    fn expression_fallback_evaluates_once() {
        let mut calls = 0;
        let value = cfg_select! {{
            any() => { undefined_value() }
            _ => {
                calls += 1;
                calls
            }
        }};
        assert_eq!(value, 1);
        assert_eq!(calls, 1);
    }

    #[test]
    fn statements_preserve_the_callers_scope() {
        cfg_select! {
            any() => { invalid Rust tokens }
            _ => { let value = VALUE; }
        }
        assert_eq!(value, 42);
    }
}
