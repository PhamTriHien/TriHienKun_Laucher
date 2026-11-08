package net.kdt.pojavlaunch;

import android.content.Context;

public final class ExitActivity {
    public interface ExitListener {
        void onJvmExited(int code, boolean isSignal);
    }

    private static volatile ExitListener listener;

    private ExitActivity() {
    }

    public static void setExitListener(ExitListener exitListener) {
        listener = exitListener;
    }

    public static void showExitMessage(Context ctx, int code, boolean isSignal) {
        ExitListener current = listener;
        if (current != null) {
            try {
                current.onJvmExited(code, isSignal);
            } catch (Throwable ignored) {
            }
        }
        com.trihienkun.laucher.GameJvmService.notifyJvmExit(code);
    }
}
