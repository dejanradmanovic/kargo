package com.example.di

import com.example.service.GreetingService
import com.example.service.GreetingServiceImpl
import dagger.Binds
import dagger.Module
import javax.inject.Singleton

@Module
abstract class AppModule {
    @Binds
    @Singleton
    abstract fun bindGreetingService(impl: GreetingServiceImpl): GreetingService
}
