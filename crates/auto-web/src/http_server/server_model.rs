//! PLAN-843 结构拆分：HTTP 服务 model 职责（原样搬移）。
use super::*;

pub use crate::vm::host_capabilities::http_service::messages::{
    ApiBody, ApiReply, ApiRequest, FileReplySeed,
};

/// CORS + JSON content-type + 请求 id（task-lock 失败臂用）。
pub(crate) fn cors_json_headers(request_id: &str) -> Vec<(String, String)> {
    let mut headers = vec![("Content-Type".to_string(), "application/json".to_string())];
    headers.extend(cors_header_pairs());
    headers.push(("X-Request-Id".to_string(), request_id.to_string()));
    headers
}

/// PLAN-776 T-04：终值 Value → NanoValue 完整值桥（sync/async 同形）。
/// - I64 全宽（48 位内联 / BigInt 堆装箱——`encode_i64_with_heap`）；
/// - VmRef 身份保持（资源凭据/普通对象——TAG_OBJECT 回编组侧）；
/// - Array/Obj 物化堆容器（id 记入 `materialized`，编组后统一释放——
///   nv_to_json/value_to_json 现成序列化 record/list）；
/// - 不可转换变体 → None（调用方 500 诊断——不再静默 null）。
pub(super) fn value_to_nv_via_task(
    vm: &std::rc::Rc<AutoVM>,
    ctx: &DispatchCtx,
    value: auto_val::Value,
    materialized: &mut Vec<u64>,
) -> Option<auto_val::NanoValue> {
    let task_id = ctx.handler_task_id?;
    let t_arc = vm.tasks.get(&task_id)?.clone();
    let mut t = t_arc.try_lock().ok()?;
    let nv = match value {
        auto_val::Value::Int(i) => auto_val::encode_i32(i),
        auto_val::Value::I64(i) => crate::vm::ffi::encode_i64_with_heap(vm, i),
        auto_val::Value::Float(f) => auto_val::encode_f64(f),
        auto_val::Value::Double(f) => auto_val::encode_f64(f),
        auto_val::Value::Bool(b) => auto_val::encode_bool(b),
        auto_val::Value::Nil | auto_val::Value::Null => auto_val::encode_null(),
        auto_val::Value::Str(s) => {
            let idx = vm.add_string(s.to_string().into_bytes());
            vm.rc_push_str_idx(&mut t, idx);
            t.ram.pop_nv()
        }
        auto_val::Value::VmRef(r) => auto_val::encode_object(r.id as u32),
        // PLAN-776 R8-01：Future 句柄 Value 载体还原 TAG_FUTURE 载荷。
        auto_val::Value::Future(f) => auto_val::encode_future(f.id as u32),
        auto_val::Value::Array(a) => {
            let list = crate::vm::types::ListData {
                elems: a.values,
                storage: None,
            };
            let id = vm.insert_heap_object(list);
            materialized.push(id);
            auto_val::encode_object(id as u32)
        }
        auto_val::Value::Obj(o) => {
            let mut od = crate::vm::types::ObjectData::new();
            for (k, v) in o.iter() {
                od.fields.insert(k.clone(), v.clone());
            }
            let id = vm.insert_heap_object(od);
            materialized.push(id);
            auto_val::encode_object(id as u32)
        }
        _ => return None,
    };
    Some(nv)
}

/// Marshal a returned NanoValue into the final JSON reply arm: PLAN-698 POST
/// broadcast + 200/500 mapping by the legacy `{"error":` prefix convention +
/// the success log line.
pub(super) fn json_value_reply(
    vm: &crate::vm::engine::AutoVM,
    nv: auto_val::NanoValue,
    req_method: &str,
    req_path: &str,
    fn_name: &str,
    request_body: &str,
    request_id: &str,
    elapsed_ms: u128,
) -> ApiReply {
    match nv_to_json(vm, nv, 0) {
        Some(result_json) => {
            // PLAN-734 T-03（D3）：`{"error":` 前缀猜测退役——业务 record
            // 含 error 字段是数据（200）；错误状态由执行/序列化失败决定。
            let status = 200;
            // PLAN-698 SD-02: POST 广播臂（Typing / New{Type}，has_sse 门控）。
            if req_method == "POST" {
                publish_post_broadcast(fn_name, request_body, &result_json);
            }
            eprintln!(
                "[HTTP] {} {} [{}] → 200 ({}ms)",
                req_method, req_path, request_id, elapsed_ms
            );
            ApiReply::Full {
                status,
                headers: cors_json_headers(request_id),
                body: ApiBody::Text(result_json.into_bytes()),
            }
        }
        // PLAN-734 T-03（D3）：序列化失败 → 500 诊断（不再 200 空 body
        // 冒充成功；AC-03）。
        None => ApiReply::Full {
            status: 500,
            headers: cors_json_headers(request_id),
            body: ApiBody::Text(br#"{"error":"response serialization failed"}"#.to_vec()),
        },
    }
}
