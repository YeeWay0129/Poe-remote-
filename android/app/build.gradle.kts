import org.gradle.api.file.DuplicatesStrategy
import org.gradle.api.tasks.Copy
import org.gradle.api.tasks.testing.Test
import java.io.File

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("org.jetbrains.kotlin.plugin.compose")
}

android {
    namespace = "com.remotepoe.app"
    compileSdk = 35

    defaultConfig {
        applicationId = "com.remotepoe.app"
        minSdk = 26
        targetSdk = 35
        versionCode = 1
        versionName = "0.1.0"

        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
    }

    buildFeatures {
        compose = true
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
}

kotlin {
    compilerOptions {
        jvmTarget.set(org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17)
    }
}

dependencies {
    implementation(platform("androidx.compose:compose-bom:2024.12.01"))
    implementation("androidx.activity:activity-compose:1.9.3")
    implementation("androidx.compose.material3:material3")
    implementation("androidx.compose.ui:ui")
    implementation("androidx.compose.ui:ui-tooling-preview")
    implementation("androidx.lifecycle:lifecycle-runtime-compose:2.8.7")
    implementation("com.squareup.okhttp3:okhttp:4.12.0")
    implementation("io.github.webrtc-sdk:android:144.7559.05")

    debugImplementation("androidx.compose.ui:ui-tooling")

    testImplementation("junit:junit:4.13.2")
    testImplementation("org.json:json:20240303")
}

val asciiDebugUnitTestRuntimeDir = File(
    System.getProperty("user.home"),
    ".gradle/remote-poe/android-debug-unit-test",
)

val prepareAsciiDebugUnitTestRuntime by tasks.registering(Copy::class) {
    dependsOn(
        "compileDebugUnitTestJavaWithJavac",
        "compileDebugUnitTestKotlin",
        "bundleDebugClassesToRuntimeJar",
    )

    duplicatesStrategy = DuplicatesStrategy.INCLUDE
    into(asciiDebugUnitTestRuntimeDir)

    from(layout.buildDirectory.dir("intermediates/javac/debugUnitTest/compileDebugUnitTestJavaWithJavac/classes")) {
        into("test-classes")
    }
    from(layout.buildDirectory.dir("tmp/kotlin-classes/debugUnitTest")) {
        into("test-classes")
    }
    from(layout.buildDirectory.file("intermediates/runtime_app_classes_jar/debug/bundleDebugClassesToRuntimeJar/classes.jar")) {
        into("main")
        rename { "app-classes.jar" }
    }

    doFirst {
        delete(asciiDebugUnitTestRuntimeDir)
    }
}

tasks.withType<Test>().configureEach {
    if (name == "testDebugUnitTest") {
        dependsOn(prepareAsciiDebugUnitTestRuntime)

        val asciiTestClasses = File(asciiDebugUnitTestRuntimeDir, "test-classes")
        val asciiAppClasses = File(asciiDebugUnitTestRuntimeDir, "main/app-classes.jar")
        testClassesDirs = files(asciiTestClasses)
        doFirst {
            classpath = files(asciiTestClasses, asciiAppClasses, classpath)
        }
    }
}
