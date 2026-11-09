#include <dlfcn.h>
#include <jni.h>
#include <malloc.h>
#include <stddef.h>

#ifndef M_BIONIC_SET_HEAP_TAGGING_LEVEL
#define M_BIONIC_SET_HEAP_TAGGING_LEVEL (-204)
#endif

#ifndef M_HEAP_TAGGING_LEVEL_NONE
#define M_HEAP_TAGGING_LEVEL_NONE 0
#endif

typedef void (*thk_update_ld_library_path_fn)(const char *);
typedef void (*thk_exit_hook_init_fn)(JNIEnv *, jclass);

__attribute__((constructor(101)))
static void thk_init_bionic_jvm_compat(void) {
    mallopt(M_BIONIC_SET_HEAP_TAGGING_LEVEL, M_HEAP_TAGGING_LEVEL_NONE);
}

JNIEXPORT jboolean JNICALL
Java_net_kdt_pojavlaunch_utils_JREUtils_nativeSafeSetLdLibraryPath(
    JNIEnv *env,
    jclass clazz,
    jstring ld_path
) {
    (void)clazz;
    if (env == NULL || ld_path == NULL) {
        return JNI_FALSE;
    }

    thk_update_ld_library_path_fn update_fn = NULL;
    void *libdl_handle = dlopen("libdl.so", RTLD_NOW | RTLD_LOCAL);
    if (libdl_handle != NULL) {
        update_fn = (thk_update_ld_library_path_fn)dlsym(
            libdl_handle,
            "android_update_LD_LIBRARY_PATH"
        );
        if (update_fn == NULL) {
            update_fn = (thk_update_ld_library_path_fn)dlsym(
                libdl_handle,
                "__loader_android_update_LD_LIBRARY_PATH"
            );
        }
    }
    if (update_fn == NULL) {
        update_fn = (thk_update_ld_library_path_fn)dlsym(
            RTLD_DEFAULT,
            "android_update_LD_LIBRARY_PATH"
        );
    }
    if (update_fn == NULL) {
        update_fn = (thk_update_ld_library_path_fn)dlsym(
            RTLD_DEFAULT,
            "__loader_android_update_LD_LIBRARY_PATH"
        );
    }
    if (update_fn == NULL) {
        return JNI_FALSE;
    }

    const char *c_path = (*env)->GetStringUTFChars(env, ld_path, NULL);
    if (c_path == NULL) {
        if ((*env)->ExceptionCheck(env)) {
            (*env)->ExceptionClear(env);
        }
        return JNI_FALSE;
    }
    update_fn(c_path);
    (*env)->ReleaseStringUTFChars(env, ld_path, c_path);
    return JNI_TRUE;
}

JNIEXPORT jboolean JNICALL
Java_net_kdt_pojavlaunch_utils_JREUtils_nativeInvokeExitHookInit(
    JNIEnv *env,
    jclass clazz,
    jstring exit_hook_path
) {
    if (env == NULL || exit_hook_path == NULL) {
        return JNI_FALSE;
    }

    const char *c_path = (*env)->GetStringUTFChars(env, exit_hook_path, NULL);
    if (c_path == NULL) {
        if ((*env)->ExceptionCheck(env)) {
            (*env)->ExceptionClear(env);
        }
        return JNI_FALSE;
    }

    void *handle = dlopen(c_path, RTLD_NOW | RTLD_GLOBAL);
    (*env)->ReleaseStringUTFChars(env, exit_hook_path, c_path);
    if (handle == NULL) {
        return JNI_FALSE;
    }

    thk_exit_hook_init_fn init_fn = (thk_exit_hook_init_fn)dlsym(
        handle,
        "Java_net_kdt_pojavlaunch_utils_JREUtils_initializeHooks"
    );
    if (init_fn == NULL) {
        return JNI_FALSE;
    }

    init_fn(env, clazz);
    if ((*env)->ExceptionCheck(env)) {
        (*env)->ExceptionClear(env);
        return JNI_FALSE;
    }
    return JNI_TRUE;
}
