package com.example.service

import com.example.config.AppConfig
import com.example.model.GitHubSearchResponse
import io.ktor.client.*
import io.ktor.client.call.*
import io.ktor.client.request.*
import org.koin.core.annotation.Single

interface GitHubService {
    suspend fun searchRepos(query: String, limit: Int? = null): List<com.example.model.GitHubRepo>
}

@Single(binds = [GitHubService::class])
class GitHubServiceImpl(
    private val client: HttpClient,
    private val config: AppConfig,
) : GitHubService {

    override suspend fun searchRepos(query: String, limit: Int?): List<com.example.model.GitHubRepo> {
        val perPage = limit ?: config.githubDefaultLimit
        val response: GitHubSearchResponse = client.get("${config.githubBaseUrl}/search/repositories") {
            parameter("q", query)
            parameter("sort", "stars")
            parameter("order", "desc")
            parameter("per_page", perPage)
            headers {
                append("Accept", "application/vnd.github.v3+json")
            }
        }.body()

        return response.items
    }
}
