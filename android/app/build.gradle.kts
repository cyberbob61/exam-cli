plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("org.jetbrains.kotlin.plugin.compose")
    id("org.jetbrains.kotlin.plugin.serialization")
}

android {
    namespace = "com.cyberbob61.exam"
    compileSdk = 35

    defaultConfig {
        applicationId = "com.cyberbob61.exam"
        minSdk = 26
        targetSdk = 35
        // CI run number, so each master build is newer than the last (the update check compares it)
        versionCode = System.getenv("GITHUB_RUN_NUMBER")?.toInt() ?: 1
        versionName = "0.4.0"
    }

    // the permanent key (CI writes it from the repo secrets); without it Gradle uses its debug key
    val keystore = System.getenv("ANDROID_KEYSTORE_FILE")
    if (keystore != null) {
        signingConfigs.create("exam") {
            storeFile = file(keystore)
            storeType = "pkcs12"
            storePassword = System.getenv("ANDROID_KEYSTORE_PASSWORD")
            keyAlias = "exam"
            keyPassword = System.getenv("ANDROID_KEYSTORE_PASSWORD")
        }
        buildTypes["debug"].signingConfig = signingConfigs["exam"]
    }

    buildFeatures {
        compose = true
        buildConfig = true
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    kotlinOptions {
        jvmTarget = "17"
    }

    // the demo exam comes from the shared samples folder of exam-cli
    sourceSets["main"].assets.srcDir("../../samples")
}

dependencies {
    val bom = platform("androidx.compose:compose-bom:2024.12.01")
    implementation(bom)
    implementation("androidx.compose.ui:ui")
    implementation("androidx.compose.material3:material3")
    implementation("androidx.compose.material:material-icons-extended")
    implementation("androidx.activity:activity-compose:1.9.3")
    implementation("androidx.core:core:1.13.1") // FileProvider for installing updates (already pulled in by activity)
    implementation("androidx.lifecycle:lifecycle-viewmodel-compose:2.8.7")
    implementation("org.jetbrains.kotlinx:kotlinx-serialization-json:1.7.3")
    testImplementation("junit:junit:4.13.2")
}
