# JNA calls into the Rust library by reflection over these classes.
-dontwarn java.awt.*
-keep class com.sun.jna.* { *; }
-keep class * extends com.sun.jna.* { *; }
# The generated code describes the functions and structures of the Rust
# library to JNA by the names and the order of its members.
-keep class publictransport.** { *; }
