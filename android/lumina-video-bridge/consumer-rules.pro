-keep class com.luminavideo.bridge.LuminaVideo {
    public static com.luminavideo.bridge.ExoPlayerBridge createPlayer(long);
}

-keep class com.luminavideo.bridge.ExoPlayerBridge {
    public void play(java.lang.String);
    public void pause();
    public void resume();
    public void seek(long);
    public void setVolume(float);
    public void setMuted(boolean);
    public long getCurrentPosition();
    public long getDuration();
    public long getPlayerId();
    public void release();
    native <methods>;
}

-keep class com.luminavideo.bridge.MoqMediaCodecBridge {
    public <init>(android.content.Context, java.lang.String, int, int, long);
    public void start();
    public void submitNalUnit(byte[], long);
    public void release();
    native <methods>;
}
