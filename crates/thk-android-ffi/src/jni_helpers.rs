use jni_sys::{
    jboolean, jchar, jclass, jdouble, jfieldID, jfloat, jint, jmethodID, jobject, jobjectArray,
    jsize, jstring, jvalue, JNIEnv, JavaVM, JavaVMAttachArgs, JNI_OK, JNI_VERSION_1_6,
};
use std::ffi::{c_void, CStr};

#[inline]
pub unsafe fn is_env_valid(env: *mut JNIEnv) -> bool {
    !env.is_null() && !(*env).is_null()
}

#[inline]
pub unsafe fn clear_pending_exception(env: *mut JNIEnv) -> bool {
    if !is_env_valid(env) {
        return false;
    }
    if ((**env).v1_6.ExceptionCheck)(env) {
        ((**env).v1_6.ExceptionClear)(env);
        true
    } else {
        false
    }
}

pub unsafe fn get_java_vm(env: *mut JNIEnv) -> Option<*mut JavaVM> {
    if !is_env_valid(env) {
        return None;
    }
    let _ = clear_pending_exception(env);
    let mut vm: *mut JavaVM = std::ptr::null_mut();
    let rc = ((**env).v1_6.GetJavaVM)(env, &mut vm);
    if clear_pending_exception(env) || rc != JNI_OK || vm.is_null() || (*vm).is_null() {
        None
    } else {
        Some(vm)
    }
}

pub unsafe fn jstring_to_string(env: *mut JNIEnv, s: jstring) -> String {
    if !is_env_valid(env) || s.is_null() {
        return String::new();
    }
    let _ = clear_pending_exception(env);
    let len: jsize = ((**env).v1_6.GetStringLength)(env, s);
    if clear_pending_exception(env) || len <= 0 {
        return String::new();
    }
    let mut buf = vec![0u16; len as usize];
    ((**env).v1_6.GetStringRegion)(env, s, 0, len, buf.as_mut_ptr());
    if clear_pending_exception(env) {
        return String::new();
    }
    String::from_utf16_lossy(&buf)
}

pub unsafe fn new_jni_string(env: *mut JNIEnv, s: &str) -> jstring {
    if !is_env_valid(env) {
        return std::ptr::null_mut();
    }
    let _ = clear_pending_exception(env);
    let utf16: Vec<jchar> = s.encode_utf16().collect();
    let empty_sentinel: [jchar; 1] = [0];
    let (ptr, len) = if utf16.is_empty() {
        (empty_sentinel.as_ptr(), 0)
    } else {
        let capped = jsize::try_from(utf16.len()).unwrap_or(jsize::MAX);
        (utf16.as_ptr(), capped)
    };
    let out = ((**env).v1_6.NewString)(env, ptr, len);
    if clear_pending_exception(env) {
        std::ptr::null_mut()
    } else {
        out
    }
}

pub unsafe fn jobject_array_to_strings(env: *mut JNIEnv, arr: jobjectArray) -> Vec<String> {
    if !is_env_valid(env) || arr.is_null() {
        return Vec::new();
    }
    let _ = clear_pending_exception(env);
    let len = ((**env).v1_6.GetArrayLength)(env, arr);
    if clear_pending_exception(env) || len <= 0 {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(len as usize);
    for idx in 0..len {
        let elem = ((**env).v1_6.GetObjectArrayElement)(env, arr, idx);
        if clear_pending_exception(env) || elem.is_null() {
            continue;
        }
        let s = jstring_to_string(env, elem);
        ((**env).v1_6.DeleteLocalRef)(env, elem);
        let _ = clear_pending_exception(env);
        out.push(s);
    }
    out
}

pub unsafe fn strings_to_jobject_array(
    env: *mut JNIEnv,
    string_cls: jclass,
    items: &[String],
) -> jobjectArray {
    if !is_env_valid(env) || string_cls.is_null() {
        return std::ptr::null_mut();
    }
    let _ = clear_pending_exception(env);
    let Ok(len) = jsize::try_from(items.len()) else {
        return std::ptr::null_mut();
    };
    let arr = ((**env).v1_6.NewObjectArray)(env, len, string_cls, std::ptr::null_mut());
    if clear_pending_exception(env) || arr.is_null() {
        return std::ptr::null_mut();
    }
    for (idx, item) in items.iter().enumerate() {
        let jstr = new_jni_string(env, item);
        if jstr.is_null() {
            ((**env).v1_6.DeleteLocalRef)(env, arr);
            let _ = clear_pending_exception(env);
            return std::ptr::null_mut();
        }
        ((**env).v1_6.SetObjectArrayElement)(env, arr, idx as jsize, jstr);
        ((**env).v1_6.DeleteLocalRef)(env, jstr);
        if clear_pending_exception(env) {
            ((**env).v1_6.DeleteLocalRef)(env, arr);
            let _ = clear_pending_exception(env);
            return std::ptr::null_mut();
        }
    }
    arr
}

pub unsafe fn find_global_class(env: *mut JNIEnv, name: &CStr) -> Option<jclass> {
    if !is_env_valid(env) {
        return None;
    }
    let _ = clear_pending_exception(env);
    let local = ((**env).v1_6.FindClass)(env, name.as_ptr());
    if clear_pending_exception(env) || local.is_null() {
        return None;
    }
    let global = ((**env).v1_6.NewGlobalRef)(env, local);
    ((**env).v1_6.DeleteLocalRef)(env, local);
    if clear_pending_exception(env) || global.is_null() {
        None
    } else {
        Some(global)
    }
}

pub unsafe fn new_global_ref(env: *mut JNIEnv, obj: jobject) -> Option<jobject> {
    if !is_env_valid(env) || obj.is_null() {
        return None;
    }
    let _ = clear_pending_exception(env);
    let global = ((**env).v1_6.NewGlobalRef)(env, obj);
    if clear_pending_exception(env) || global.is_null() {
        None
    } else {
        Some(global)
    }
}

pub unsafe fn delete_global_ref(env: *mut JNIEnv, global: jobject) {
    if is_env_valid(env) && !global.is_null() {
        let _ = clear_pending_exception(env);
        ((**env).v1_6.DeleteGlobalRef)(env, global);
        let _ = clear_pending_exception(env);
    }
}

#[inline]
pub unsafe fn delete_local_ref(env: *mut JNIEnv, local: jobject) {
    if is_env_valid(env) && !local.is_null() {
        ((**env).v1_6.DeleteLocalRef)(env, local);
    }
}

pub unsafe fn get_method_id(
    env: *mut JNIEnv,
    cls: jclass,
    name: &CStr,
    sig: &CStr,
) -> Option<jmethodID> {
    if !is_env_valid(env) || cls.is_null() {
        return None;
    }
    let _ = clear_pending_exception(env);
    let id = ((**env).v1_6.GetMethodID)(env, cls, name.as_ptr(), sig.as_ptr());
    if clear_pending_exception(env) || id.is_null() {
        None
    } else {
        Some(id)
    }
}

pub unsafe fn get_static_method_id(
    env: *mut JNIEnv,
    cls: jclass,
    name: &CStr,
    sig: &CStr,
) -> Option<jmethodID> {
    if !is_env_valid(env) || cls.is_null() {
        return None;
    }
    let _ = clear_pending_exception(env);
    let id = ((**env).v1_6.GetStaticMethodID)(env, cls, name.as_ptr(), sig.as_ptr());
    if clear_pending_exception(env) || id.is_null() {
        None
    } else {
        Some(id)
    }
}

pub unsafe fn get_static_field_id(
    env: *mut JNIEnv,
    cls: jclass,
    name: &CStr,
    sig: &CStr,
) -> Option<jfieldID> {
    if !is_env_valid(env) || cls.is_null() {
        return None;
    }
    let _ = clear_pending_exception(env);
    let id = ((**env).v1_6.GetStaticFieldID)(env, cls, name.as_ptr(), sig.as_ptr());
    if clear_pending_exception(env) || id.is_null() {
        None
    } else {
        Some(id)
    }
}

#[inline]
pub unsafe fn call_void_method(
    env: *mut JNIEnv,
    obj: jobject,
    method: jmethodID,
    args: &[jvalue],
) -> bool {
    if !is_env_valid(env) || obj.is_null() || method.is_null() {
        return false;
    }
    let _ = clear_pending_exception(env);
    let empty: [jvalue; 0] = [];
    let ptr = if args.is_empty() {
        empty.as_ptr()
    } else {
        args.as_ptr()
    };
    ((**env).v1_6.CallVoidMethodA)(env, obj, method, ptr);
    !clear_pending_exception(env)
}

#[inline]
pub unsafe fn call_boolean_method(
    env: *mut JNIEnv,
    obj: jobject,
    method: jmethodID,
    args: &[jvalue],
) -> Option<jboolean> {
    if !is_env_valid(env) || obj.is_null() || method.is_null() {
        return None;
    }
    let _ = clear_pending_exception(env);
    let empty: [jvalue; 0] = [];
    let ptr = if args.is_empty() {
        empty.as_ptr()
    } else {
        args.as_ptr()
    };
    let res = ((**env).v1_6.CallBooleanMethodA)(env, obj, method, ptr);
    if clear_pending_exception(env) {
        None
    } else {
        Some(res)
    }
}

#[inline]
pub unsafe fn call_static_void_method(
    env: *mut JNIEnv,
    cls: jclass,
    method: jmethodID,
    args: &[jvalue],
) -> bool {
    if !is_env_valid(env) || cls.is_null() || method.is_null() {
        return false;
    }
    let _ = clear_pending_exception(env);
    let empty: [jvalue; 0] = [];
    let ptr = if args.is_empty() {
        empty.as_ptr()
    } else {
        args.as_ptr()
    };
    ((**env).v1_6.CallStaticVoidMethodA)(env, cls, method, ptr);
    !clear_pending_exception(env)
}

#[inline]
pub unsafe fn call_static_boolean_method(
    env: *mut JNIEnv,
    cls: jclass,
    method: jmethodID,
    args: &[jvalue],
) -> Option<jboolean> {
    if !is_env_valid(env) || cls.is_null() || method.is_null() {
        return None;
    }
    let _ = clear_pending_exception(env);
    let empty: [jvalue; 0] = [];
    let ptr = if args.is_empty() {
        empty.as_ptr()
    } else {
        args.as_ptr()
    };
    let res = ((**env).v1_6.CallStaticBooleanMethodA)(env, cls, method, ptr);
    if clear_pending_exception(env) {
        None
    } else {
        Some(res)
    }
}

#[inline]
pub unsafe fn call_static_int_method(
    env: *mut JNIEnv,
    cls: jclass,
    method: jmethodID,
    args: &[jvalue],
) -> Option<jint> {
    if !is_env_valid(env) || cls.is_null() || method.is_null() {
        return None;
    }
    let _ = clear_pending_exception(env);
    let empty: [jvalue; 0] = [];
    let ptr = if args.is_empty() {
        empty.as_ptr()
    } else {
        args.as_ptr()
    };
    let res = ((**env).v1_6.CallStaticIntMethodA)(env, cls, method, ptr);
    if clear_pending_exception(env) {
        None
    } else {
        Some(res)
    }
}

#[inline]
pub unsafe fn set_static_int_field(env: *mut JNIEnv, cls: jclass, field: jfieldID, val: jint) {
    if is_env_valid(env) && !cls.is_null() && !field.is_null() {
        let _ = clear_pending_exception(env);
        ((**env).v1_6.SetStaticIntField)(env, cls, field, val);
        let _ = clear_pending_exception(env);
    }
}

#[inline]
pub unsafe fn get_static_int_field(env: *mut JNIEnv, cls: jclass, field: jfieldID) -> jint {
    if !is_env_valid(env) || cls.is_null() || field.is_null() {
        return 0;
    }
    let _ = clear_pending_exception(env);
    let val = ((**env).v1_6.GetStaticIntField)(env, cls, field);
    if clear_pending_exception(env) {
        0
    } else {
        val
    }
}

#[inline]
pub unsafe fn set_static_float_field(env: *mut JNIEnv, cls: jclass, field: jfieldID, val: jfloat) {
    if is_env_valid(env) && !cls.is_null() && !field.is_null() {
        let _ = clear_pending_exception(env);
        ((**env).v1_6.SetStaticFloatField)(env, cls, field, val);
        let _ = clear_pending_exception(env);
    }
}

#[inline]
pub const fn jval_bool(v: jboolean) -> jvalue {
    jvalue { z: v }
}

#[inline]
pub const fn jval_char(v: jchar) -> jvalue {
    jvalue { c: v }
}

#[inline]
pub const fn jval_int(v: jint) -> jvalue {
    jvalue { i: v }
}

#[inline]
pub const fn jval_float(v: jfloat) -> jvalue {
    jvalue { f: v }
}

#[inline]
pub const fn jval_double(v: jdouble) -> jvalue {
    jvalue { d: v }
}

#[inline]
pub const fn jval_obj(v: jobject) -> jvalue {
    jvalue { l: v }
}

pub struct AttachedThreadGuard {
    vm: *mut JavaVM,
    env: *mut JNIEnv,
    needs_detach: bool,
}

impl AttachedThreadGuard {
    pub unsafe fn attach(vm: *mut JavaVM, thread_name: &CStr, daemon: bool) -> Option<Self> {
        if vm.is_null() || (*vm).is_null() {
            return None;
        }
        let mut raw_env: *mut c_void = std::ptr::null_mut();
        let existing_rc = ((**vm).v1_4.GetEnv)(vm, &mut raw_env, JNI_VERSION_1_6);
        if existing_rc == JNI_OK && !raw_env.is_null() {
            let env = raw_env.cast::<JNIEnv>();
            if is_env_valid(env) {
                return Some(Self {
                    vm,
                    env,
                    needs_detach: false,
                });
            }
        }

        let mut attach_args = JavaVMAttachArgs {
            version: JNI_VERSION_1_6,
            name: thread_name.as_ptr().cast_mut(),
            group: std::ptr::null_mut(),
        };
        raw_env = std::ptr::null_mut();
        let attach_ptr = std::ptr::addr_of_mut!(attach_args).cast::<c_void>();
        let rc = if daemon {
            ((**vm).v1_4.AttachCurrentThreadAsDaemon)(vm, &mut raw_env, attach_ptr)
        } else {
            ((**vm).v1_4.AttachCurrentThread)(vm, &mut raw_env, attach_ptr)
        };
        if rc != JNI_OK || raw_env.is_null() {
            return None;
        }
        let env = raw_env.cast::<JNIEnv>();
        if !is_env_valid(env) {
            let _ = ((**vm).v1_4.DetachCurrentThread)(vm);
            return None;
        }
        Some(Self {
            vm,
            env,
            needs_detach: true,
        })
    }

    #[inline]
    pub fn env(&self) -> *mut JNIEnv {
        self.env
    }
}

impl Drop for AttachedThreadGuard {
    fn drop(&mut self) {
        if self.needs_detach && !self.vm.is_null() {
            unsafe {
                let _ = clear_pending_exception(self.env);
                if !(*self.vm).is_null() {
                    let _ = ((**self.vm).v1_4.DetachCurrentThread)(self.vm);
                }
            }
        }
    }
}
