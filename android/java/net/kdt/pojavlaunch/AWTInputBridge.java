package net.kdt.pojavlaunch;

public final class AWTInputBridge {
    private AWTInputBridge() {
    }

    public static native void nativeSendData(int type, int i1, int i2, int i3, int i4);

    public static native void nativeClipboardReceived(String data, String mimeTypeSub);

    public static native void nativeMoveWindow(int xoff, int yoff);
}
