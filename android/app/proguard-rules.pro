-keep class com.zhongbai233.epub.reader.model.** { *; }
-keepattributes *Annotation*
-dontwarn org.jsoup.**

# ONNX Runtime — uses JNI; keep all native-bound classes/methods.
-keep class ai.onnxruntime.** { *; }
-keepclasseswithmembernames class * { native <methods>; }

# OkHttp / Okio (used by Edge TTS WebSocket) — silence reflective lookups.
-dontwarn okhttp3.internal.platform.**
-dontwarn org.conscrypt.**
-dontwarn org.bouncycastle.**
-dontwarn org.openjsse.**

# kotlinx.serialization — keep @Serializable companion objects.
-keepattributes RuntimeVisibleAnnotations,AnnotationDefault
-keep,includedescriptorclasses class com.zhongbai233.epub.reader.**$$serializer { *; }
-keepclassmembers class com.zhongbai233.epub.reader.** {
    *** Companion;
}
-keepclasseswithmembers class com.zhongbai233.epub.reader.** {
    kotlinx.serialization.KSerializer serializer(...);
}
