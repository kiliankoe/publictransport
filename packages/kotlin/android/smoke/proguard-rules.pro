# What the test calls. Everything else is up to R8 and the rules of the library.
-keep class io.github.kiliankoe.publictransport.smoke.Smoke { *; }
# The test libraries take what they share with the app from the app: the
# Kotlin library and the AndroidX classes that OkHttp brings.
-keep class kotlin.** { *; }
-keep class androidx.** { *; }
