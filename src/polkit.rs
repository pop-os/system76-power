use dbus::{
    arg::{RefArg, Variant},
    nonblock::{Proxy, SyncConnection},
    strings::BusName,
};
use std::{
    collections::HashMap,
    time::Duration,
};

type AuthorizationResult<'l> = (bool, bool, HashMap<String, String>);
type SubjectDetails<'l> = HashMap<&'l str, Variant<Box<dyn RefArg>>>;
type Subject<'l> = (&'l str, SubjectDetails<'l>);
type Details<'l> = HashMap<&'l str, &'l str>;

const ALLOW_USER_INTERACTION: u32 = 1;

pub(crate) async fn get_connection_unix_process_id<'a>(
    c: &SyncConnection,
    sender: BusName<'a>,
) -> Result<u32, dbus::Error> {
    let proxy = Proxy::new(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        Duration::new(25, 0),
        c,
    );
    let (pid,) = proxy.method_call(
        "org.freedesktop.DBus",
        "GetConnectionUnixProcessID",
        (sender.to_string(),),
    ).await?;
    Ok(pid)
}

pub(crate) async fn check_authorization(
    c: &SyncConnection,
    pid: u32,
    start_time: u64,
    action_id: &str,
) -> Result<bool, dbus::Error> {
    let proxy = Proxy::new(
        "org.freedesktop.PolicyKit1",
        "/org/freedesktop/PolicyKit1/Authority",
        Duration::new(25, 0),
        c,
    );

    let mut subject_details = SubjectDetails::new();
    subject_details.insert("pid", Variant(Box::new(pid)));
    subject_details.insert("start-time", Variant(Box::new(start_time)));
    let subject: Subject = ("unix-process", subject_details);

    let args = (subject, action_id, Details::new(), ALLOW_USER_INTERACTION, "");
    let ((is_authorized, _is_challenge, _details),): (AuthorizationResult,) =
        proxy.method_call("org.freedesktop.PolicyKit1.Authority", "CheckAuthorization", args).await?;
    Ok(is_authorized)
}
