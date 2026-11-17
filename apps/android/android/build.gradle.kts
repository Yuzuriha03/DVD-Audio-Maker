allprojects {
    repositories {
        maven { url = uri("https://storage.flutter-io.cn/download.flutter.io") }
        maven { url = uri("https://mirrors.huaweicloud.com/repository/maven/") }
        maven { url = uri("https://maven.aliyun.com/repository/google") }
        maven { url = uri("https://maven.aliyun.com/repository/central") }
    }
    configurations.configureEach {
        resolutionStrategy.eachDependency {
            val group = requested.group.orEmpty()
            val flutterLocal = group.startsWith("io.flutter") ||
                group == "com.github.dart_lang.jni" ||
                group == "com.jvoltci.saf"
            if (!requested.version.isNullOrBlank() && !flutterLocal) {
                useVersion("latest.release")
            }
        }
    }
}

val newBuildDir: Directory =
    rootProject.layout.buildDirectory
        .dir("../../build")
        .get()
rootProject.layout.buildDirectory.value(newBuildDir)

subprojects {
    val newSubprojectBuildDir: Directory = newBuildDir.dir(project.name)
    project.layout.buildDirectory.value(newSubprojectBuildDir)
}
subprojects {
    project.evaluationDependsOn(":app")
}

tasks.register<Delete>("clean") {
    delete(rootProject.layout.buildDirectory)
}
