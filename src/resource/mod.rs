pub mod drawing;
pub mod ip;
pub mod location;
pub mod network;
pub mod room_cabinets;
pub mod switch;

// 重新导出 network 模块中的所有函数，以便直接从 crate::resource 导入
pub use network::*;
// 重新导出 switch 模块中的所有函数
pub use switch::*;
// 重新导出 drawing 模块中的所有函数
pub use drawing::*;
// 重新导出 location 模块中的所有函数
pub use location::*;
// 重新导出 room_cabinets 模块中的所有函数
pub use room_cabinets::*;
// 重新导出 ip 模块中的所有函数
pub use ip::*;
