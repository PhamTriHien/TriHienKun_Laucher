package org.lwjgl.glfw;

import android.content.ClipData;
import android.content.ClipDescription;
import android.content.ClipboardManager;
import android.content.Context;
import android.content.Intent;
import android.net.Uri;
import android.util.DisplayMetrics;

import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.nio.FloatBuffer;

public final class CallbackBridge {
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

    private static volatile Context appContext;

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

    public static boolean notifyLauncher(int type, int... action) {
        return false;
    }

    public static void onDirectInputEnable() {
    }

    public static void onGrabStateChanged(boolean grabbing) {
        com.trihienkun.laucher.ThkNativeBridge.nativeNotifyGrabState(grabbing);
    }

    public static float getAndroidDPI() {
        DisplayMetrics metrics = new DisplayMetrics();
        metrics.setToDefaults();
        return metrics.density * scaleFactor;
    }

    public static native void nativeSetUseInputStackQueue(boolean useInputStackQueue);

    public static native boolean nativeSendChar(char codepoint);

    public static native boolean nativeSendCharMods(char codepoint, int mods);

    public static native void nativeSendKey(int key, int scancode, int action, int mods);

    public static native void nativeSendCursorPos(float x, float y);

    public static native void nativeSendMouseButton(int button, int action, int mods);

    public static native void nativeSendScroll(double xoffset, double yoffset);

    public static native void nativeSendScreenSize(int width, int height);

    public static native void nativeSetWindowAttrib(int attrib, int value);

    public static native ByteBuffer nativeCreateGamepadButtonBuffer();

    public static native ByteBuffer nativeCreateGamepadAxisBuffer();
}
