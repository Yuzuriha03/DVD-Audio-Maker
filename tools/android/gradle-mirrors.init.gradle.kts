import org.gradle.api.artifacts.repositories.ArtifactRepository
import org.gradle.api.artifacts.repositories.MavenArtifactRepository
import java.net.URI

fun ArtifactRepository.rewriteOfficialRepository() {
    if (this !is MavenArtifactRepository) return
    val mirror = when (url.host?.lowercase()) {
        "maven.google.com", "dl.google.com" ->
            "https://maven.aliyun.com/repository/google"
        "repo.maven.apache.org", "repo1.maven.org" ->
            "https://maven.aliyun.com/repository/central"
        "plugins.gradle.org" ->
            "https://maven.aliyun.com/repository/gradle-plugin"
        "download.flutter.io", "storage.googleapis.com" ->
            "https://storage.flutter-io.cn/download.flutter.io"
        else -> null
    }
    if (mirror != null) {
        url = URI(mirror)
    }
}

beforeSettings {
    pluginManagement.repositories {
        clear()
        maven { url = uri("https://maven.aliyun.com/repository/google") }
        maven { url = uri("https://maven.aliyun.com/repository/central") }
        maven { url = uri("https://maven.aliyun.com/repository/gradle-plugin") }
        all { rewriteOfficialRepository() }
    }
}

settingsEvaluated {
    dependencyResolutionManagement.repositories {
        clear()
        maven { url = uri("https://storage.flutter-io.cn/download.flutter.io") }
        maven { url = uri("https://maven.aliyun.com/repository/google") }
        maven { url = uri("https://maven.aliyun.com/repository/central") }
        all { rewriteOfficialRepository() }
    }
}

allprojects {
    buildscript.repositories {
        maven { url = uri("https://maven.aliyun.com/repository/google") }
        maven { url = uri("https://maven.aliyun.com/repository/central") }
        maven { url = uri("https://maven.aliyun.com/repository/gradle-plugin") }
        all { rewriteOfficialRepository() }
    }
    // Do not add project repositories here: Flutter's included build forbids them.
    // Rewrite repositories contributed by the app and third-party plugins instead.
    repositories.all { rewriteOfficialRepository() }
}
