package net.kdt.pojavlaunch;

public final class Tools {
    private Tools() {
    }

    public static final class SDL {
        private SDL() {
        }

        public static native void initializeControllerSubsystems();
    }
}
