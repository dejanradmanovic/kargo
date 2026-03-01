package com.example.di

import com.example.service.GreetingService
import dagger.Component
import javax.inject.Singleton

@Singleton
@Component(modules = [AppModule::class])
interface AppComponent {
    fun greetingService(): GreetingService
}
