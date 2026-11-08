package net.kdt.pojavlaunch;

import org.lwjgl.glfw.CallbackBridge;

public final class MainActivity {
    private MainActivity() {
    }

    @SuppressWarnings("unused")
    public static void openLink(String link) {
        CallbackBridge.accessAndroidClipboard(CallbackBridge.CLIPBOARD_OPEN, link);
    }

    @SuppressWarnings("unused")
    public static void querySystemClipboard() {
        String clip = CallbackBridge.accessAndroidClipboard(CallbackBridge.CLIPBOARD_PASTE, null);
        AWTInputBridge.nativeClipboardReceived(clip != null ? clip : "", "plain");
    }

    @SuppressWarnings("unused")
    public static void putClipboardData(String data, String mimeType) {
        if (data != null) {
            CallbackBridge.accessAndroidClipboard(CallbackBridge.CLIPBOARD_COPY, data);
        }
    }
}
