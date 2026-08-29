# Keep UniFFI-generated Kotlin (it is loaded reflectively by JNI).
-keep class morselink_core.** { *; }
-dontwarn morselink_core.**

# Media3 reflects on player components.
-keep class androidx.media3.** { *; }
-dontwarn androidx.media3.**
