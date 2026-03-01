package com.example.di

import com.example.config.AppConfig
import io.ktor.client.*
import io.ktor.client.engine.cio.*
import io.ktor.client.plugins.contentnegotiation.*
import io.ktor.serialization.kotlinx.json.*
import kotlinx.serialization.json.Json
import org.koin.core.annotation.ComponentScan
import org.koin.core.annotation.Module
import org.koin.core.annotation.Single

@Module
@ComponentScan("com.example")
class AppModule {

    @Single
    fun provideAppConfig(): AppConfig = AppConfig.load()

    @Single
    fun provideJson(): Json = Json {
        ignoreUnknownKeys = true
        prettyPrint = true
    }

    @Single
    fun provideHttpClient(json: Json): HttpClient = HttpClient(CIO) {
        install(ContentNegotiation) {
            json(json)
        }
    }
}
