package org.lwjgl.glfw;

import android.content.ClipData;
import android.content.ClipDescription;
import android.content.ClipboardManager;
import android.content.Context;
import android.content.Intent;
import android.net.Uri;
import android.os.Handler;
import android.os.Looper;
import android.util.DisplayMetrics;

import net.kdt.pojavlaunch.LwjglGlfwKeycode;
import net.kdt.pojavlaunch.utils.JREUtils;

import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.nio.FloatBuffer;
import java.util.ArrayList;
import java.util.List;

public final class CallbackBridge {
    public interface GrabListener {
        void onGrabState(boolean isGrabbing);
    }

    public static final int CLIPBOARD_COPY = 2000;
    public static final int CLIPBOARD_PASTE = 2001;
    public static final int CLIPBOARD_OPEN = 2002;

    public static volatile int windowWidth;
    public static volatile int windowHeight;
    public static volatile int physicalWidth;
    public static volatile int physicalHeight;
    public static volatile float scaleFactor = 1.0f;
    public static volatile float mouseX;
    public static volatile float mouseY;
    public static volatile boolean holdingAlt;
    public static volatile boolean holdingCapslock;
    public static volatile boolean holdingCtrl;
    public static volatile boolean holdingNumlock;
    public static volatile boolean holdingShift;

    private static volatile boolean isGrabbing = false;
    private static volatile Context appContext;
    private static final List<GrabListener> grabListeners = new ArrayList<>();
    private static final Handler MAIN_HANDLER = new Handler(Looper.getMainLooper());

    public static final ByteBuffer sGamepadButtonBuffer;
    public static final FloatBuffer sGamepadAxisBuffer;

    static {
        ByteBuffer btns = null;
        FloatBuffer axes = null;
        try {
            btns = nativeCreateGamepadButtonBuffer();
            ByteBuffer rawAxes = nativeCreateGamepadAxisBuffer();
            if (rawAxes != null) {
                axes = rawAxes.order(ByteOrder.LITTLE_ENDIAN).asFloatBuffer();
            }
        } catch (Throwable ignored) {
        }
        sGamepadButtonBuffer = btns;
        sGamepadAxisBuffer = axes;
    }

    private CallbackBridge() {
    }

    public static void attachContext(Context ctx) {
        if (ctx != null) {
            appContext = ctx.getApplicationContext();
        }
    }

    public static void sendCursorPos(float x, float y) {
        mouseX = x;
        mouseY = y;
        if (!JREUtils.isBridgeLoaded()) {
            return;
        }
        nativeSendCursorPos(x, y);
    }

    public static void sendKeycode(int keycode, char keychar, int scancode, int modifiers, boolean isDown) {
        if (keycode != 0) {
            setModifiers(keycode, isDown);
            if (JREUtils.isBridgeLoaded()) {
                nativeSendKey(keycode, scancode, isDown ? 1 : 0, getCurrentMods() | modifiers);
            }
        }
        if (isDown && keychar != '\u0000' && !Character.isISOControl(keychar) && JREUtils.isBridgeLoaded()) {
            int mods = getCurrentMods() | modifiers;
            nativeSendCharMods(keychar, mods);
            nativeSendChar(keychar);
        }
    }

    public static void sendKeyPress(int keyCode, boolean isDown) {
        sendKeycode(keyCode, '\u0000', 0, getCurrentMods(), isDown);
    }

    public static void sendKeyTap(int keyCode) {
        sendKeyPress(keyCode, true);
        sendKeyPress(keyCode, false);
    }

    public static void sendChar(char keychar, int modifiers) {
        if (!JREUtils.isBridgeLoaded()) {
            return;
        }
        nativeSendCharMods(keychar, modifiers);
        nativeSendChar(keychar);
    }

    public static void sendMouseButton(int button, boolean isDown) {
        if (!JREUtils.isBridgeLoaded()) {
            return;
        }
        nativeSendMouseButton(button, isDown ? 1 : 0, getCurrentMods());
    }

    public static void sendMouseTap(int button) {
        sendMouseButton(button, true);
        MAIN_HANDLER.postDelayed(() -> sendMouseButton(button, false), 28L);
    }

    public static void sendScroll(double xoffset, double yoffset) {
        if (!JREUtils.isBridgeLoaded()) {
            return;
        }
        nativeSendScroll(xoffset, yoffset);
    }

    public static void sendUpdateWindowSize(int w, int h) {
        windowWidth = w;
        windowHeight = h;
        if (!JREUtils.isBridgeLoaded()) {
            return;
        }
        nativeSendScreenSize(w, h);
    }

    public static boolean isGrabbing() {
        return isGrabbing;
    }

    public static void addGrabListener(GrabListener listener) {
        if (listener == null) {
            return;
        }
        synchronized (grabListeners) {
            listener.onGrabState(isGrabbing);
            if (!grabListeners.contains(listener)) {
                grabListeners.add(listener);
            }
        }
    }

    public static void removeGrabListener(GrabListener listener) {
        synchronized (grabListeners) {
            grabListeners.remove(listener);
        }
    }

    public static int getCurrentMods() {
        int mods = 0;
        if (holdingAlt) {
            mods |= LwjglGlfwKeycode.GLFW_MOD_ALT;
        }
        if (holdingCapslock) {
            mods |= LwjglGlfwKeycode.GLFW_MOD_CAPS_LOCK;
        }
        if (holdingCtrl) {
            mods |= LwjglGlfwKeycode.GLFW_MOD_CONTROL;
        }
        if (holdingNumlock) {
            mods |= LwjglGlfwKeycode.GLFW_MOD_NUM_LOCK;
        }
        if (holdingShift) {
            mods |= LwjglGlfwKeycode.GLFW_MOD_SHIFT;
        }
        return mods;
    }

    public static void setModifiers(int keyCode, boolean isDown) {
        switch (keyCode) {
            case LwjglGlfwKeycode.GLFW_KEY_LEFT_SHIFT:
            case LwjglGlfwKeycode.GLFW_KEY_RIGHT_SHIFT:
                holdingShift = isDown;
                break;
            case LwjglGlfwKeycode.GLFW_KEY_LEFT_CONTROL:
            case LwjglGlfwKeycode.GLFW_KEY_RIGHT_CONTROL:
                holdingCtrl = isDown;
                break;
            case LwjglGlfwKeycode.GLFW_KEY_LEFT_ALT:
            case LwjglGlfwKeycode.GLFW_KEY_RIGHT_ALT:
                holdingAlt = isDown;
                break;
            case LwjglGlfwKeycode.GLFW_KEY_CAPS_LOCK:
                holdingCapslock = isDown;
                break;
            case LwjglGlfwKeycode.GLFW_KEY_NUM_LOCK:
                holdingNumlock = isDown;
                break;
            default:
                break;
        }
    }

    @SuppressWarnings("unused")
    public static String accessAndroidClipboard(int type, String copy) {
        Context ctx = appContext;
        if (ctx == null) {
            return "";
        }
        try {
            ClipboardManager cm = (ClipboardManager) ctx.getSystemService(Context.CLIPBOARD_SERVICE);
            switch (type) {
                case CLIPBOARD_COPY:
                    if (cm != null && copy != null) {
                        cm.setPrimaryClip(ClipData.newPlainText("Minecraft", copy));
                    }
                    return null;
                case CLIPBOARD_PASTE:
                    if (cm != null
                        && cm.hasPrimaryClip()
                        && cm.getPrimaryClipDescription() != null
                        && cm.getPrimaryClipDescription().hasMimeType(ClipDescription.MIMETYPE_TEXT_PLAIN)
                        && cm.getPrimaryClip() != null
                        && cm.getPrimaryClip().getItemCount() > 0) {
                        CharSequence text = cm.getPrimaryClip().getItemAt(0).getText();
                        return text != null ? text.toString() : "";
                    }
                    return "";
                case CLIPBOARD_OPEN:
                    if (copy != null && (copy.startsWith("http://") || copy.startsWith("https://"))) {
                        Intent intent = new Intent(Intent.ACTION_VIEW, Uri.parse(copy));
                        intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
                        ctx.startActivity(intent);
                    }
                    return null;
                default:
                    return null;
            }
        } catch (Throwable ignored) {
            return "";
        }
    }

    @SuppressWarnings("unused")
    public static boolean notifyLauncher(int type, int... action) {
        return false;
    }

    @SuppressWarnings("unused")
    private static void onDirectInputEnable() {
    }

    @SuppressWarnings("unused")
    private static void onGrabStateChanged(final boolean grabbing) {
        isGrabbing = grabbing;
        try {
            MAIN_HANDLER.postDelayed(() -> {
                if (isGrabbing != grabbing) {
                    return;
                }
                List<GrabListener> snapshot;
                synchronized (grabListeners) {
                    snapshot = new ArrayList<>(grabListeners);
                }
                for (GrabListener g : snapshot) {
                    g.onGrabState(grabbing);
                }
            }, 16L);
        } catch (Throwable ignored) {
        }
    }

    @SuppressWarnings("unused")
    private static float getAndroidDPI() {
        DisplayMetrics metrics = new DisplayMetrics();
        metrics.setToDefaults();
        return metrics.density * scaleFactor;
    }

    public static native void nativeSetUseInputStackQueue(boolean useInputStackQueue);

    private static native boolean nativeSendChar(char codepoint);

    private static native boolean nativeSendCharMods(char codepoint, int mods);

    private static native void nativeSendKey(int key, int scancode, int action, int mods);

    private static native void nativeSendCursorPos(float x, float y);

    private static native void nativeSendMouseButton(int button, int action, int mods);

    private static native void nativeSendScroll(double xoffset, double yoffset);

    private static native void nativeSendScreenSize(int width, int height);

    public static native void nativeSetWindowAttrib(int attrib, int value);

    private static native ByteBuffer nativeCreateGamepadButtonBuffer();

    private static native ByteBuffer nativeCreateGamepadAxisBuffer();
}
