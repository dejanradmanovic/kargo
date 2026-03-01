package com.example.config

import java.util.Properties

class AppConfig private constructor(private val props: Properties) {

    val appName: String get() = props.getProperty("app.name", "unknown")
    val appVersion: String get() = props.getProperty("app.version", "0.0.0")
    val githubBaseUrl: String get() = props.getProperty("github.api.base-url", "https://api.github.com")
    val githubDefaultLimit: Int get() = props.getProperty("github.api.default-limit", "5").toInt()

    companion object {
        fun load(resource: String = "application.properties"): AppConfig {
            val props = Properties()
            val stream = AppConfig::class.java.classLoader.getResourceAsStream(resource)
                ?: throw IllegalStateException("Resource '$resource' not found on classpath")
            stream.use { props.load(it) }
            return AppConfig(props)
        }
    }
}
