use std::cell::RefCell;
use std::ptr;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::time::Duration;

use napi::bindgen_prelude::FromNapiValue;
use napi::sys::{
    napi_env, napi_get_and_clear_last_exception, napi_is_exception_pending, napi_value,
    Status as NapiStatus,
};
use napi::threadsafe_function::{ErrorStrategy, ThreadsafeFunction, ThreadsafeFunctionCallMode};
use napi::{Env, JsFunction, JsObject, JsUnknown, Status, ValueType};
use uc_engine::{HostCapabilityError, HostCapabilityErrorCategory};

const HOST_CALLBACK_TIMEOUT: Duration = Duration::from_secs(5);

struct ReturnedValue {
    env: Env,
    value: JsUnknown,
}

impl FromNapiValue for ReturnedValue {
    unsafe fn from_napi_value(env: napi_env, value: napi_value) -> napi::Result<Self> {
        // 两个句柄只在 N-API 回调的 JS 线程内使用，不进入原生等待线程。
        Ok(Self {
            env: unsafe { Env::from_raw(env) },
            value: unsafe { JsUnknown::from_napi_value(env, value)? },
        })
    }
}

pub(super) fn call_host<T, D, F>(
    callback: &ThreadsafeFunction<T, ErrorStrategy::Fatal>,
    value: T,
    convert: F,
) -> Result<D, HostCapabilityError>
where
    T: 'static,
    D: Send + 'static,
    F: FnOnce(JsObject) -> Result<D, HostCapabilityError> + 'static,
{
    let (sender, receiver) = mpsc::sync_channel(1);
    let expired = Arc::new(AtomicBool::new(false));
    let callback_expired = Arc::clone(&expired);
    let status = callback.call_with_return_value::<ReturnedValue, _>(
        value,
        ThreadsafeFunctionCallMode::NonBlocking,
        move |returned| {
            let completion = Rc::new(RefCell::new(Some((sender.clone(), convert))));
            let attach = (|| -> napi::Result<()> {
                if !returned.value.is_promise()? {
                    if !callback_expired.load(Ordering::Acquire) {
                        if let Some((sender, convert)) = completion.borrow_mut().take() {
                            let _ =
                                sender.send(convert_result(returned.env, returned.value, convert));
                        }
                    }
                    return Ok(());
                }
                let fulfilled = Rc::clone(&completion);
                let expired = Arc::clone(&callback_expired);
                let resolve =
                    returned
                        .env
                        .create_function_from_closure("hostResolved", move |ctx| {
                            let pending = fulfilled.borrow_mut().take();
                            if let Some((sender, convert)) = pending {
                                if !expired.load(Ordering::Acquire) {
                                    let result = match ctx.get::<JsUnknown>(0) {
                                        Ok(value) => convert_result(*ctx.env, value, convert),
                                        Err(_) => Err(callback_error()),
                                    };
                                    let _ = sender.send(result);
                                }
                            }
                            Ok(())
                        })?;
                let rejected = Rc::clone(&completion);
                let reject =
                    returned
                        .env
                        .create_function_from_closure("hostRejected", move |_ctx| {
                            if let Some((sender, _)) = rejected.borrow_mut().take() {
                                // 不读取 Promise 的拒绝正文，避免把宿主敏感信息送入错误或日志。
                                let _ = sender.send(Err(callback_error()));
                            }
                            Ok(())
                        })?;
                // is_promise 已确认对象类型，句柄仍处于当前 JS 回调作用域。
                let promise: JsObject = unsafe { returned.value.cast() };
                let then: JsFunction = promise.get_named_property("then")?;
                then.call(Some(&promise), &[resolve, reject])?;
                Ok(())
            })();
            let had_exception = clear_pending_exception(returned.env);
            if attach.is_err() || had_exception {
                completion.borrow_mut().take();
                let _ = sender.send(Err(callback_error()));
            }
            Ok(())
        },
    );
    if status != Status::Ok {
        return Err(callback_error());
    }
    let result = receiver.recv_timeout(HOST_CALLBACK_TIMEOUT);
    expired.store(true, Ordering::Release);
    // discarded-source[channel]: 通道错误只表示回调未完成，不保存可能携带内容的负载。
    result.map_err(|_| callback_error())?
}

fn convert_result<D, F>(env: Env, value: JsUnknown, convert: F) -> Result<D, HostCapabilityError>
where
    F: FnOnce(JsObject) -> Result<D, HostCapabilityError>,
{
    let result = match value.get_type() {
        Ok(ValueType::Object) => {
            // 类型已确认，所有属性读取和字节复制均在当前 JS 线程完成。
            convert(unsafe { value.cast() })
        }
        _ => Err(callback_error()),
    };
    if clear_pending_exception(env) {
        return Err(callback_error());
    }
    result
}

fn clear_pending_exception(env: Env) -> bool {
    let mut pending = false;
    // env 来自当前 N-API 回调；异常值只在此作用域清除，不跨线程或输出。
    if unsafe { napi_is_exception_pending(env.raw(), &mut pending) } != NapiStatus::napi_ok {
        return true;
    }
    if pending {
        let mut exception = ptr::null_mut();
        unsafe { napi_get_and_clear_last_exception(env.raw(), &mut exception) };
    }
    pending
}

fn callback_error() -> HostCapabilityError {
    HostCapabilityError::new(
        HostCapabilityErrorCategory::Unavailable,
        "HarmonyOS host callback unavailable",
    )
}
