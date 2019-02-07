// Defines whether the value returned by the method should be appended.
macro_rules! append {
    (has_out, $m:ident, $value:ident) => {
        $m.msg.method_return().append1($value)
    };

    (no_out, $m:ident, $value:ident) => {
        $m.msg.method_return()
    };
}

// Programs the message that should be printed.
macro_rules! get_value {
    (has_in, $name:expr, $daemon:ident, $m:ident, $method:tt) => {{
        let value = $m.msg.read1()?;
        info!("DBUS Received {}({}) method", $name, value);
        $daemon.borrow_mut().$method(value)
    }};

    (no_in, $name:expr, $daemon:ident, $m:ident, $method:tt) => {{
        info!("DBUS Received {} method", $name);
        $daemon.borrow_mut().$method()
    }};
}

#[macro_export]
macro_rules! dbus_impl {
    ($daemon:expr, $f:ident {
        $(
            fn $method:tt<$name:tt, $append:tt, $hasvalue:tt>(
                $( $inarg_name:tt : $inarg_type:ty ),*
            ) $( -> $($outarg_name:tt: $outarg_type:ty ),* )*;
        )*

        $(
            signal $signal:ident;
        )*
    }) => {{
        let interface = $f.interface(DBUS_IFACE, ())
            $(
                .add_m({
                    let daemon = $daemon.clone();
                    $f.method(stringify!($name), (), move |m| {
                        let result = get_value!($hasvalue, stringify!($name), daemon, m, $method);
                        match result {
                            Ok(_value) => {
                                let mret = append!($append, m, _value);
                                Ok(vec![mret])
                            }
                            Err(err) => {
                                error!("{}", err);
                                Err(MethodErr::failed(&err))
                            }
                        }
                    })
                    $(.inarg::<$inarg_type, _>(stringify!($inarg_name)))*
                    $($(.outarg::<$outarg_type, _>(stringify!($outarg_name)))*)*
                })
            )*
            $(
                .add_s($signal.clone())
            )*;

        $f.tree(()).add($f.object_path(DBUS_PATH, ()).introspectable().add(interface))
    }}
}
