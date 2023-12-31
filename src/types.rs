// //types.rs
//
// use std::fmt;
//
// #[derive(Clone)]
// pub struct MessageWithWSID {
//     pub connection_id: String,
//     pub content: String,
// }
//
//
// impl fmt::Debug for MessageWithWSID {
//     fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
//         write!(f, "MessageWithWSID {{ connection_id: {}, content: [{}] }}", self.connection_id, self.content)
//     }
// }