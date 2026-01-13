// 引入内核必要的 Rust 绑定
use kernel::prelude::*;
use kernel::sync::Mutex;
use kernel::collections::HashMap;
use kernel::str::CStr;

// 定义键值对的类型（简化版：键为字符串，值为 u64）
type KVKey = &'static CStr;
type KVValue = u64;

// 全局 KV 存储实例（用 Mutex 保证线程安全，内核态必须加锁）
static KV_STORE: Mutex<HashMap<KVKey, KVValue>> = Mutex::new(HashMap::new());

/// 内核态 KV 存储函数
/// key: 字符串键（CStr 兼容内核字符串格式）
/// value: 64 位无符号整数值
fn kv_set(key: KVKey, value: KVValue) -> Result<(), &'static str> {
    // 获取锁（内核态 Mutex，自动处理死锁检测）
    let mut store = KV_STORE.lock();
    // 插入/更新键值对
    store.insert(key, value);
    Ok(())
}

/// 内核态 KV 查询函数
/// key: 字符串键
/// 返回：Option<KVValue>（存在则返回值，不存在则返回 None）
fn kv_get(key: KVKey) -> Option<KVValue> {
    let store = KV_STORE.lock();
    store.get(&key).copied()
}

// 内核模块初始化函数（模块加载时执行）
#[init]
fn kv_module_init() -> Result<(), &'static str> {
    pr_info!("KV module initialized (Rust)\n");

    // 测试：插入初始 KV 对
    let test_key = CStr::from_bytes_with_nul(b"test_key\0").unwrap();
    kv_set(test_key, 12345)?;
    pr_info!("Inserted test_key: 12345\n");

    // 测试：查询 KV 对
    if let Some(val) = kv_get(test_key) {
        pr_info!("Queried test_key: {}\n", val);
    } else {
        pr_err!("test_key not found!\n");
    }

    Ok(())
}

// 内核模块退出函数（模块卸载时执行）
#[exit]
fn kv_module_exit() {
    pr_info!("KV module exiting (Rust)\n");
    // 清空 KV 存储
    let mut store = KV_STORE.lock();
    store.clear();
    pr_info!("KV store cleared\n");
}

// 注册内核模块（指定名称、初始化/退出函数）
module! {
    name: "rust_kv",
    init: kv_module_init,
    exit: kv_module_exit,
    license: "GPL", // 内核模块必须声明 GPL 许可证
    author: "Your Name",
    description: "Rust KV store for Linux kernel",
    version: "1.0",
}