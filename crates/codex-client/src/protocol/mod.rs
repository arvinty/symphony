pub mod v1;

#[allow(clippy::all, dead_code, non_snake_case, non_camel_case_types, unused_imports)]
pub mod v2 {
    include!(concat!(env!("OUT_DIR"), "/v2.rs"));
}

pub mod messages;
