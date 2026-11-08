package net.kdt.pojavlaunch;

public final class Logger {
    public interface eventLogListener {
        void onEventLogged(String text);
    }

    private Logger() {
    }

    public static native void begin(String logFilePath);

    public static native void appendToLog(String text);

    public static native void setLogListener(eventLogListener listener);
}
