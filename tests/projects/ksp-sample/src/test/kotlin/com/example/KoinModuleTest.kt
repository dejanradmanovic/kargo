package com.example

import com.example.di.AppModule
import com.example.service.GitHubService
import org.koin.core.context.startKoin
import org.koin.core.context.stopKoin
import org.koin.ksp.generated.module
import kotlin.test.Test
import kotlin.test.assertNotNull

class KoinModuleTest {

    @Test
    fun `koin module resolves all dependencies`() {
        val app = startKoin {
            modules(AppModule().module)
        }

        try {
            val service = app.koin.get<GitHubService>()
            assertNotNull(service)
        } finally {
            stopKoin()
        }
    }
}
