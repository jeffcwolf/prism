pub fn get() {}
pub fn post() {}
pub fn put() {}
pub fn delete() {}
pub fn patch() {}
pub fn head() {}
pub fn options() {}
pub struct Config;
pub enum Method { Get, Post }
pub trait Handler {}
pub type Result = std::result::Result<(), ()>;
