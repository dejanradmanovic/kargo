package com.example

import com.example.config.AppConfig
import com.example.di.AppModule
import com.example.service.GitHubService
import kotlinx.coroutines.runBlocking
import org.koin.core.context.startKoin
import org.koin.core.context.stopKoin
import org.koin.ksp.generated.module

fun main() = runBlocking {
    val app = startKoin {
        modules(AppModule().module)
    }

    val config = app.koin.get<AppConfig>()
    val gitHubService = app.koin.get<GitHubService>()

    println("=== ${config.appName} v${config.appVersion} ===")
    println("Searching GitHub for Kotlin repositories...\n")

    try {
        val repos = gitHubService.searchRepos("kotlin")

        repos.forEachIndexed { index, repo ->
            println("${index + 1}. ${repo.fullName}")
            println("   ★ ${repo.stars}  |  ${repo.language ?: "N/A"}")
            repo.description?.let { println("   $it") }
            println("   ${repo.url}")
            println()
        }
    } catch (e: Exception) {
        System.err.println("Failed to fetch repositories: ${e.message}")
    } finally {
        stopKoin()
    }
}
