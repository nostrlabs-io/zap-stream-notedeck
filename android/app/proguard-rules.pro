-keep class stream.zap.app.MainActivity {
    boolean openUrl(java.lang.String);
    java.lang.String getClipboardText();
    void setClipboardText(java.lang.String);
    boolean amberRequest(java.lang.String);
    void scheduleLiveCheck(boolean);
    void setNowPlaying(java.lang.String, java.lang.String);
    void setLandscapeFullscreen(boolean);
    void enterPip();
    native <methods>;
}

-keep class stream.zap.app.Nip55Signer {
    native <methods>;
}

-keep class stream.zap.app.LiveCheckWorker {
    public <init>(android.content.Context, androidx.work.WorkerParameters);
    native <methods>;
}

-keep class com.google.androidgamesdk.GameActivity {
    void finish();
    void setWindowFlags(int, int);
    androidx.core.graphics.Insets getWindowInsets(int);
    androidx.core.graphics.Insets getWaterfallInsets();
    void setImeEditorInfoFields(int, int, int);
    native <methods>;
}

-keep class com.google.androidgamesdk.gametextinput.State {
    <init>(java.lang.String, int, int, int, int);
    java.lang.String text;
    int selectionStart;
    int selectionEnd;
    int composingRegionStart;
    int composingRegionEnd;
}

-keep class com.google.androidgamesdk.gametextinput.InputConnection {
    void setState(com.google.androidgamesdk.gametextinput.State);
    void setSoftKeyboardActive(boolean, int);
    void restartInput();
}

-keep class androidx.core.graphics.Insets {
    int left;
    int top;
    int right;
    int bottom;
}

-keep class androidx.core.view.WindowInsetsCompat$Type {
    public static int captionBar();
    public static int displayCutout();
    public static int ime();
    public static int mandatorySystemGestures();
    public static int navigationBars();
    public static int statusBars();
    public static int systemBars();
    public static int systemGestures();
    public static int tappableElement();
}
